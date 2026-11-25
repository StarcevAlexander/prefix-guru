use postcss_guru::process;
use prefix_guru::{Autoprefixer, Options};

#[test]
fn readme_quick_start_holds() {
    let prefixer = Autoprefixer::new(Options::default()).unwrap();
    let output = prefixer
        .process("a { user-select: none; transition: transform 1s }")
        .unwrap();
    assert_eq!(
        output.css,
        "a { -webkit-user-select: none; -moz-user-select: none; user-select: none; transition: transform 1s }"
    );
}

#[test]
fn readme_plugin_example_holds() {
    let prefixer = Autoprefixer::new(Options {
        browsers: Some(vec![
            "ie 10".into(),
            "ie 11".into(),
            "safari 5.1".into(),
            "android 4".into(),
        ]),
        ..Options::default()
    })
    .unwrap();
    let output = process("::placeholder { color: red }", &[Box::new(prefixer)]).unwrap();
    assert_eq!(
        output.css,
        "::-webkit-input-placeholder { color: red }\n:-ms-input-placeholder { color: red }\n::placeholder { color: red }"
    );
}

#[test]
fn readme_browsers_example_holds() {
    let prefixer = Autoprefixer::new(Options {
        browsers: Some(vec!["chrome 40".into(), "safari 8".into(), "ie 11".into()]),
        ..Options::default()
    })
    .unwrap();
    assert_eq!(
        prefixer.browsers(),
        &[
            "chrome 40".to_string(),
            "ie 11".to_string(),
            "safari 8".to_string()
        ]
    );
}

#[test]
fn readme_warning_example_holds() {
    let prefixer = Autoprefixer::new(Options::default()).unwrap();
    let output = prefixer
        .process("a { background: linear-gradient(left, white, black) }")
        .unwrap();
    assert_eq!(
        output.messages[0].text,
        "Gradient has outdated direction syntax. New syntax is like `to left` instead of `right`."
    );
}

#[test]
fn readme_defaults_and_browserslist_hold() {
    let defaults = prefix_guru::defaults();
    assert!(defaults.contains(&"chrome 145".to_string()));
    assert!(defaults.contains(&"firefox 147".to_string()));
    assert!(defaults.contains(&"op_mini all".to_string()));

    let one = prefix_guru::browserslist(&["last 1 version"]).unwrap();
    assert!(one.contains(&"ie 11".to_string()));
    assert!(one.contains(&"op_mini all".to_string()));

    let error = prefix_guru::browserslist(&["electron 20"]).unwrap_err();
    assert!(error.0.contains("Electron"));
}

#[test]
fn readme_grid_error_holds() {
    let prefixer = Autoprefixer::new(Options::default()).unwrap();
    let error = prefixer
        .process("/* autoprefixer grid: on */\na { grid-template-columns: 1fr 1fr }")
        .unwrap_err();
    assert!(matches!(error, prefix_guru::AutoprefixError::Grid(_)));
    assert!(error.to_string().contains("IE Grid translation"));
}
