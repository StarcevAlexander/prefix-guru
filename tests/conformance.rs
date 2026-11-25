use serde_json::Value;

fn options_for(run: &Value) -> prefix_guru::Options {
    let opts = &run["opts"];
    let flexbox = match opts.get("flexbox") {
        Some(Value::String(text)) if text == "no-2009" => prefix_guru::Flexbox::No2009,
        Some(Value::Bool(false)) => prefix_guru::Flexbox::None,
        _ => prefix_guru::Flexbox::All,
    };
    prefix_guru::Options {
        browsers: run["browsers"].as_array().map(|list| {
            list.iter()
                .map(|value| value.as_str().unwrap().to_string())
                .collect()
        }),
        add: opts.get("add").and_then(Value::as_bool).unwrap_or(true),
        remove: opts.get("remove").and_then(Value::as_bool).unwrap_or(true),
        cascade: opts.get("cascade").and_then(Value::as_bool).unwrap_or(true),
        flexbox,
        supports: opts
            .get("supports")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    }
}

#[test]
fn conformance() {
    let data: Value =
        serde_json::from_str(include_str!("conformance.json")).expect("fixture parses");
    let mut css_failures = 0;
    let mut warn_failures = 0;
    let mut runs = 0;
    for case in data.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let css = case["css"].as_str().unwrap();
        for run in case["runs"].as_array().unwrap() {
            if run.get("error").is_some() {
                continue;
            }
            let key = run["key"].as_str().unwrap();
            runs += 1;
            let output = match prefix_guru::process(css, options_for(run)) {
                Ok(output) => output,
                Err(error) => {
                    css_failures += 1;
                    println!("{name} [{key}]: process error {error}");
                    continue;
                }
            };
            let expected_css = run["out"].as_str().unwrap();
            if output.css != expected_css {
                css_failures += 1;
                if css_failures <= 12 {
                    let mut at = 0;
                    while at < expected_css.len()
                        && at < output.css.len()
                        && expected_css.as_bytes()[at] == output.css.as_bytes()[at]
                    {
                        at += 1;
                    }
                    println!(
                        "{name} [{key}] css differs at byte {at}:\n  expected {:?}\n  mine     {:?}",
                        &expected_css[at.saturating_sub(60)..(at + 90).min(expected_css.len())],
                        &output.css[at.saturating_sub(60)..(at + 90).min(output.css.len())],
                    );
                }
            }
            let expected_warnings = run["warnings"].as_array().unwrap();
            if output.messages.len() != expected_warnings.len() {
                warn_failures += 1;
                if warn_failures <= 12 {
                    println!(
                        "{name} [{key}]: {} warnings vs expected {}",
                        output.messages.len(),
                        expected_warnings.len()
                    );
                    for message in &output.messages {
                        println!("  mine: {message:?}");
                    }
                    for warning in expected_warnings {
                        println!("  want: {warning}");
                    }
                }
                continue;
            }
            for (mine, expected) in output.messages.iter().zip(expected_warnings) {
                let line = expected["line"].as_u64();
                let column = expected["column"].as_u64();
                let end_line = expected["endLine"].as_u64();
                let end_column = expected["endColumn"].as_u64();
                if mine.text != expected["text"].as_str().unwrap()
                    || mine.line.map(u64::from) != line
                    || mine.column.map(u64::from) != column
                    || mine.end_line.map(u64::from) != end_line
                    || mine.end_column.map(u64::from) != end_column
                {
                    warn_failures += 1;
                    if warn_failures <= 12 {
                        println!("{name} [{key}]:\n  mine   {mine:?}\n  want   {expected}");
                    }
                }
            }
        }
    }
    println!(
        "conformance runs {runs} css failures {css_failures} warning failures {warn_failures}"
    );
    assert_eq!(css_failures, 0, "css divergences");
    assert_eq!(warn_failures, 0, "warning divergences");
}
