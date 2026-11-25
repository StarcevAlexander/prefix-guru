use serde_json::Value;

#[test]
fn dev_queries() {
    let Ok(path) = std::env::var("PG_ORACLE") else {
        return;
    };
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    let mut ok = 0;
    for case in data["queries"].as_array().unwrap() {
        let label = case["query"].as_str().unwrap();
        let queries: Vec<&str> = match case.get("multi") {
            Some(list) => list
                .as_array()
                .unwrap()
                .iter()
                .map(|q| q.as_str().unwrap())
                .collect(),
            None => vec![label],
        };
        match (prefix_guru::browserslist(&queries), case.get("error")) {
            (Ok(actual), None) => {
                let expected: Vec<String> = case["result"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                if actual == expected {
                    ok += 1;
                } else {
                    let missing: Vec<String> = expected
                        .iter()
                        .filter(|v| !actual.contains(v))
                        .cloned()
                        .collect();
                    let extra: Vec<String> = actual
                        .iter()
                        .filter(|v| !expected.contains(v))
                        .cloned()
                        .collect();
                    failures.push(format!("{label}: missing {missing:?} extra {extra:?}"));
                }
            }
            (Ok(actual), Some(error)) => failures.push(format!(
                "{label}: expected error {:?}, got {} results",
                error,
                actual.len()
            )),
            (Err(error), None) => failures.push(format!("{label}: unexpected error {}", error.0)),
            (Err(error), Some(expected)) => {
                if error.0 != expected.as_str().unwrap() {
                    failures.push(format!(
                        "{label}: error text {:?} vs {:?}",
                        error.0, expected
                    ));
                }
            }
        }
    }
    println!("queries ok {ok} failures {}", failures.len());
    for failure in failures.iter().take(40) {
        println!("{failure}");
    }
    assert!(failures.is_empty(), "browserslist divergences");
}

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
fn dev_ap() {
    let Ok(path) = std::env::var("PG_AP") else {
        return;
    };
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
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
            if output.css != run["out"].as_str().unwrap() {
                css_failures += 1;
                if css_failures <= 60 {
                    let expected = run["out"].as_str().unwrap();
                    let mut at = 0;
                    while at < expected.len()
                        && at < output.css.len()
                        && expected.as_bytes()[at] == output.css.as_bytes()[at]
                    {
                        at += 1;
                    }
                    println!(
                        "{name} [{key}] css differs at byte {at}:\n  expected {:?}\n  mine     {:?}",
                        &expected[at.saturating_sub(60)..(at + 90).min(expected.len())],
                        &output.css[at.saturating_sub(60)..(at + 90).min(output.css.len())],
                    );
                }
            }
            let expected_warnings = run["warnings"].as_array().unwrap();
            if output.messages.len() != expected_warnings.len() {
                warn_failures += 1;
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
                    if warn_failures <= 20 {
                        println!("{name} [{key}]:\n  mine   {mine:?}\n  want   {expected}");
                    }
                }
            }
        }
    }
    println!("ap runs {runs} css failures {css_failures} warning failures {warn_failures}");
    assert_eq!(css_failures, 0, "css divergences");
    assert_eq!(warn_failures, 0, "warning divergences");
}
