use std::sync::OnceLock;

pub(crate) fn prefix(text: &str) -> String {
    static PREFIX: OnceLock<regex::Regex> = OnceLock::new();
    let regex = PREFIX.get_or_init(|| regex::Regex::new(r"^(-[0-9A-Za-z_]+-)").unwrap());
    regex
        .captures(text)
        .map_or_else(String::new, |captures| captures[0].to_string())
}

pub(crate) fn unprefixed(text: &str) -> String {
    static PREFIX: OnceLock<regex::Regex> = OnceLock::new();
    let regex = PREFIX.get_or_init(|| regex::Regex::new(r"^-[0-9A-Za-z_]+-").unwrap());
    regex.replace(text, "").into_owned()
}
