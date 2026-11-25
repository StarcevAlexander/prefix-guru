use regex::Regex;

pub(crate) const JS_SPACE: &str =
    r" \t\n\x0B\x0C\r\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000\uFEFF";

pub(crate) fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{b}'
            | '\u{c}'
            | '\r'
            | ' '
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    ) || ('\u{2000}'..='\u{200a}').contains(&c)
}

pub(crate) fn js_trim(text: &str) -> String {
    text.trim_matches(is_js_space).to_string()
}

pub(crate) fn uniq<T: Clone + PartialEq>(list: &[T]) -> Vec<T> {
    let mut unique: Vec<T> = Vec::new();
    for item in list {
        if !unique.contains(item) {
            unique.push(item.clone());
        }
    }
    unique
}

pub(crate) fn remove_note(text: &str) -> &str {
    text.split(' ').next().unwrap_or(text)
}

pub(crate) fn escape_regexp(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "$()*+-.?[\\]^{|}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub(crate) fn regexp(word: &str, escape: bool) -> Regex {
    let word = if escape {
        escape_regexp(word)
    } else {
        word.to_string()
    };
    Regex::new(&format!(
        r"(?i)(^|[{JS_SPACE}(,])({word}($|[{JS_SPACE}(,]))"
    ))
    .unwrap()
}

pub(crate) fn comma(value: &str) -> Vec<String> {
    split(value, &[","], true)
}

pub(crate) fn space(value: &str) -> Vec<String> {
    split(value, &[" ", "\n", "\t"], false)
}

fn split(value: &str, separators: &[&str], last: bool) -> Vec<String> {
    let letters: Vec<char> = value.chars().collect();
    let mut list: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut split = false;
    let mut func = 0usize;
    let mut in_quote = false;
    let mut prev_quote = ' ';
    let mut escape = false;
    let mut in_comment = false;
    let mut index = 0;
    while let Some(letter) = letters.get(index).copied() {
        if in_comment {
            current.push(letter);
            if letter == '*' && letters.get(index + 1) == Some(&'/') {
                current.push('/');
                index += 1;
                in_comment = false;
            }
        } else {
            if escape {
                escape = false;
            } else if letter == '\\' {
                escape = true;
            } else if in_quote {
                if letter == prev_quote {
                    in_quote = false;
                }
            } else if letter == '"' || letter == '\'' {
                in_quote = true;
                prev_quote = letter;
            } else if letter == '/' && letters.get(index + 1) == Some(&'*') {
                current.push_str("/*");
                index += 1;
                in_comment = true;
                index += 1;
                continue;
            } else if letter == '(' {
                func += 1;
            } else if letter == ')' {
                func = func.saturating_sub(1);
            } else if func == 0
                && separators
                    .iter()
                    .any(|separator| separator.contains(letter))
            {
                split = true;
            }
            if split {
                let value = js_trim(&current);
                if last || !value.is_empty() {
                    list.push(value);
                }
                current.clear();
                split = false;
            } else {
                current.push(letter);
            }
        }
        index += 1;
    }
    let value = js_trim(&current);
    if last || !value.is_empty() {
        list.push(value);
    }
    list
}

pub(crate) fn split_whitespace(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_js_space(c) {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

pub(crate) fn edit_list(
    value: &str,
    callback: impl FnOnce(Vec<String>, &mut Vec<String>) -> Option<Vec<String>>,
) -> String {
    let mut prefixed = Vec::new();
    let changed = callback(comma(value), &mut prefixed);
    let Some(changed) = changed else {
        return value.to_string();
    };
    let separator = Regex::new(r",\s*")
        .unwrap()
        .find(value)
        .map_or(", ", |found| found.as_str());
    changed.join(separator)
}

pub(crate) fn collapse_first_spaces(text: &str) -> String {
    let regex = Regex::new(r"\s+").unwrap();
    regex.replace(text, " ").into_owned()
}

pub(crate) fn number(value: f64) -> String {
    if value.is_nan() {
        return String::from("NaN");
    }
    if value == 0.0 {
        return String::from("0");
    }
    let magnitude = value.abs();
    let scientific = format!("{magnitude:e}");
    let (digits, power) = scientific
        .split_once('e')
        .map_or((scientific.as_str(), 0), |(digits, power)| {
            (digits, power.parse::<i32>().unwrap_or(0))
        });
    let sign = if value < 0.0 { "-" } else { "" };
    if !(-6..21).contains(&power) {
        return format!(
            "{sign}{digits}e{}{power}",
            if power < 0 { '-' } else { '+' }
        );
    }
    format!("{value}")
}

pub(crate) fn parse_float(text: &str) -> f64 {
    let text = text.trim_start_matches(is_js_space);
    let bytes = text.as_bytes();
    let mut at = usize::from(bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+'));
    let mut digits = 0;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
        digits += 1;
    }
    if at < bytes.len() && bytes[at] == b'.' {
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        if text.starts_with("Infinity") || text.starts_with("+Infinity") {
            return f64::INFINITY;
        }
        if text.starts_with("-Infinity") {
            return f64::NEG_INFINITY;
        }
        return f64::NAN;
    }
    let mut end = at;
    if at < bytes.len() && (bytes[at] == b'e' || bytes[at] == b'E') {
        let mut exp = at + 1;
        if exp < bytes.len() && (bytes[exp] == b'-' || bytes[exp] == b'+') {
            exp += 1;
        }
        if exp < bytes.len() && bytes[exp].is_ascii_digit() {
            while exp < bytes.len() && bytes[exp].is_ascii_digit() {
                exp += 1;
            }
            end = exp;
        }
    }
    text[..end].parse().unwrap_or(f64::NAN)
}

pub(crate) fn parse_int(text: &str) -> f64 {
    let text = text.trim_start_matches(is_js_space);
    let bytes = text.as_bytes();
    let at = usize::from(bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+'));
    let mut end = at;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == at {
        return f64::NAN;
    }
    text[..end].parse().unwrap_or(f64::NAN)
}
