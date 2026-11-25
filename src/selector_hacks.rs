use crate::selectors::{Base, SelectorHack};

struct Autofill;

impl SelectorHack for Autofill {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        map_prefixes_all(prefixes, "-webkit-")
    }

    fn prefixed(&self, _name: &str, prefix: &str) -> String {
        if prefix == "-webkit-" {
            return String::from(":-webkit-autofill");
        }
        format!(":{prefix}autofill")
    }
}

struct Fullscreen;

impl SelectorHack for Fullscreen {
    fn prefixed(&self, _name: &str, prefix: &str) -> String {
        match prefix {
            "-webkit-" => String::from(":-webkit-full-screen"),
            "-moz-" => String::from(":-moz-full-screen"),
            other => format!(":{other}fullscreen"),
        }
    }
}

struct Placeholder;

impl SelectorHack for Placeholder {
    fn possible(&self) -> Vec<String> {
        let mut possible = Base.possible();
        possible.push(String::from("-moz- old"));
        possible.push(String::from("-ms- old"));
        possible
    }

    fn prefixed(&self, _name: &str, prefix: &str) -> String {
        match prefix {
            "-webkit-" => String::from("::-webkit-input-placeholder"),
            "-ms-" => String::from("::-ms-input-placeholder"),
            "-ms- old" => String::from(":-ms-input-placeholder"),
            "-moz- old" => String::from(":-moz-placeholder"),
            other => format!("::{other}placeholder"),
        }
    }
}

struct PlaceholderShown;

impl SelectorHack for PlaceholderShown {
    fn prefixed(&self, _name: &str, prefix: &str) -> String {
        match prefix {
            "-moz-" => String::from(":-moz-placeholder"),
            "-ms-" => String::from(":-ms-input-placeholder"),
            other => format!(":{other}placeholder-shown"),
        }
    }
}

struct FileSelectorButton;

impl SelectorHack for FileSelectorButton {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        map_prefixes_all(prefixes, "-webkit-")
    }

    fn prefixed(&self, _name: &str, prefix: &str) -> String {
        if prefix == "-webkit-" {
            return String::from("::-webkit-file-upload-button");
        }
        format!("::{prefix}file-selector-button")
    }
}

fn map_prefixes_all(prefixes: Vec<String>, to: &str) -> Vec<String> {
    if prefixes.is_empty() {
        return prefixes;
    }
    let mapped: Vec<String> = prefixes.iter().map(|_| to.to_string()).collect();
    crate::utils::uniq(&mapped)
}

pub(crate) static SELECTOR_HACKS: &[(&[&str], &dyn SelectorHack)] = &[
    (&[":autofill"], &Autofill),
    (&[":fullscreen"], &Fullscreen),
    (&["::placeholder"], &Placeholder),
    (&[":placeholder-shown"], &PlaceholderShown),
    (&["::file-selector-button"], &FileSelectorButton),
];
