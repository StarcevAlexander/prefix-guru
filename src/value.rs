const OPEN: u16 = 40;
const CLOSE: u16 = 41;
const SINGLE_QUOTE: u16 = 39;
const DOUBLE_QUOTE: u16 = 34;
const BACKSLASH: u16 = 92;
const SLASH: u16 = 47;
const COMMA: u16 = 44;
const COLON: u16 = 58;
const STAR: u16 = 42;
const U_LOWER: u16 = 117;
const U_UPPER: u16 = 85;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ValueNode {
    Word(String),
    Space(String),
    Str {
        quote: char,
        value: String,
        unclosed: bool,
    },
    Comment {
        value: String,
        unclosed: bool,
    },
    Div {
        value: String,
        before: String,
        after: String,
    },
    UnicodeRange(String),
    Function {
        value: String,
        before: String,
        after: String,
        unclosed: bool,
        nodes: Vec<ValueNode>,
    },
}

impl ValueNode {
    pub fn value(&self) -> &str {
        match self {
            Self::Word(value) | Self::Space(value) | Self::UnicodeRange(value) => value,
            Self::Str { value, .. } | Self::Comment { value, .. } => value,
            Self::Div { value, .. } | Self::Function { value, .. } => value,
        }
    }

    pub fn set_value(&mut self, text: impl Into<String>) {
        let text = text.into();
        match self {
            Self::Word(value) | Self::Space(value) | Self::UnicodeRange(value) => *value = text,
            Self::Str { value, .. } | Self::Comment { value, .. } => *value = text,
            Self::Div { value, .. } | Self::Function { value, .. } => *value = text,
        }
    }

    pub fn is_word(&self, text: &str) -> bool {
        matches!(self, Self::Word(value) if value == text)
    }

    pub fn is_function(&self, name: &str) -> bool {
        matches!(self, Self::Function { value, .. } if value == name)
    }

    pub fn nodes(&self) -> Option<&[ValueNode]> {
        match self {
            Self::Function { nodes, .. } => Some(nodes),
            _ => None,
        }
    }
}

pub(crate) fn stringify(nodes: &[ValueNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        stringify_into(node, &mut out);
    }
    out
}

pub(crate) fn stringify_node(node: &ValueNode) -> String {
    let mut out = String::new();
    stringify_into(node, &mut out);
    out
}

fn stringify_into(node: &ValueNode, out: &mut String) {
    match node {
        ValueNode::Word(value) | ValueNode::Space(value) | ValueNode::UnicodeRange(value) => {
            out.push_str(value)
        }
        ValueNode::Str {
            quote,
            value,
            unclosed,
        } => {
            out.push(*quote);
            out.push_str(value);
            if !unclosed {
                out.push(*quote);
            }
        }
        ValueNode::Comment { value, unclosed } => {
            out.push_str("/*");
            out.push_str(value);
            if !unclosed {
                out.push_str("*/");
            }
        }
        ValueNode::Div {
            value,
            before,
            after,
        } => {
            out.push_str(before);
            out.push_str(value);
            out.push_str(after);
        }
        ValueNode::Function {
            value,
            before,
            after,
            unclosed,
            nodes,
        } => {
            out.push_str(value);
            out.push('(');
            out.push_str(before);
            out.push_str(&stringify(nodes));
            out.push_str(after);
            if !unclosed {
                out.push(')');
            }
        }
    }
}

fn is_space(code: Option<u16>) -> bool {
    code.is_some_and(|code| code <= 32)
}

fn is_unicode_range(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '?' || c == '-')
}

struct ValueParser {
    value: Vec<u16>,
    pos: usize,
    code: Option<u16>,
    root: Vec<ValueNode>,
    stack: Vec<ValueNode>,
    balanced: usize,
    name: String,
    before: String,
    after: String,
}

impl ValueParser {
    fn code_at(&self, at: usize) -> Option<u16> {
        self.value.get(at).copied()
    }

    fn slice(&self, from: usize, to: usize) -> String {
        let from = from.min(self.value.len());
        let to = to.min(self.value.len());
        String::from_utf16_lossy(&self.value[from..to.max(from)])
    }

    fn tokens_mut(&mut self) -> &mut Vec<ValueNode> {
        match self.stack.last_mut() {
            Some(ValueNode::Function { nodes, .. }) => nodes,
            _ => &mut self.root,
        }
    }

    fn last_is_div(&self) -> bool {
        let tokens = match self.stack.last() {
            Some(ValueNode::Function { nodes, .. }) => nodes,
            _ => &self.root,
        };
        matches!(tokens.last(), Some(ValueNode::Div { .. }))
    }

    fn in_calc(&self) -> bool {
        matches!(self.stack.last(), Some(ValueNode::Function { value, .. }) if value == "calc")
    }

    fn escaped(&self, mut escape_pos: usize) -> bool {
        let mut escape = false;
        while escape_pos > 0 && self.code_at(escape_pos - 1) == Some(BACKSLASH) {
            escape_pos -= 1;
            escape = !escape;
        }
        escape
    }

    fn skip_spaces(&mut self, mut next: usize) -> usize {
        loop {
            next += 1;
            self.code = self.code_at(next);
            if !is_space(self.code) {
                return next;
            }
        }
    }

    fn run(&mut self) {
        let max = self.value.len();
        while self.pos < max {
            let pos = self.pos;
            let code = self.code;
            if is_space(code) {
                let next = self.skip_spaces(pos);
                let token = self.slice(pos, next);
                let closing = self.code == Some(CLOSE) && self.balanced > 0;
                let starts_div = self.code == Some(COMMA)
                    || self.code == Some(COLON)
                    || (self.code == Some(SLASH)
                        && self.code_at(next + 1) != Some(STAR)
                        && !self.in_calc());
                if closing {
                    self.after = token;
                } else if self.last_is_div() {
                    if let Some(ValueNode::Div { after, .. }) = self.tokens_mut().last_mut() {
                        *after = token;
                    }
                } else if starts_div {
                    self.before = token;
                } else {
                    self.tokens_mut().push(ValueNode::Space(token));
                }
                self.pos = next;
            } else if code == Some(SINGLE_QUOTE) || code == Some(DOUBLE_QUOTE) {
                let quote = if code == Some(SINGLE_QUOTE) {
                    '\''
                } else {
                    '"'
                };
                let mut next = pos;
                let mut unclosed = false;
                loop {
                    let mut escape = false;
                    match find(&self.value, next + 1, code.unwrap()) {
                        Some(at) => {
                            escape = self.escaped(at);
                            next = at;
                        }
                        None => {
                            self.value.push(code.unwrap());
                            next = self.value.len() - 1;
                            unclosed = true;
                        }
                    }
                    if !escape {
                        break;
                    }
                }
                let text = self.slice(pos + 1, next);
                self.tokens_mut().push(ValueNode::Str {
                    quote,
                    value: text,
                    unclosed,
                });
                self.pos = next + 1;
                self.code = self.code_at(self.pos);
            } else if code == Some(SLASH) && self.code_at(pos + 1) == Some(STAR) {
                let found = find_pair(&self.value, pos, STAR, SLASH);
                let (next, unclosed) = match found {
                    Some(at) => (at, false),
                    None => {
                        let next = self.value.len();
                        (next, true)
                    }
                };
                let text = self.slice(pos + 2, next);
                self.tokens_mut().push(ValueNode::Comment {
                    value: text,
                    unclosed,
                });
                self.pos = next + 2;
                self.code = self.code_at(self.pos);
            } else if (code == Some(SLASH) || code == Some(STAR)) && self.in_calc() {
                let token = self.slice(pos, pos + 1);
                self.tokens_mut().push(ValueNode::Word(token));
                self.pos = pos + 1;
                self.code = self.code_at(self.pos);
            } else if code == Some(SLASH) || code == Some(COMMA) || code == Some(COLON) {
                let token = self.slice(pos, pos + 1);
                let before = std::mem::take(&mut self.before);
                self.tokens_mut().push(ValueNode::Div {
                    value: token,
                    before,
                    after: String::new(),
                });
                self.pos = pos + 1;
                self.code = self.code_at(self.pos);
            } else if code == Some(OPEN) {
                let next = self.skip_spaces(pos);
                let parentheses_open = pos;
                let name = std::mem::take(&mut self.name);
                let mut function = ValueNode::Function {
                    value: name,
                    before: self.slice(parentheses_open + 1, next),
                    after: String::new(),
                    unclosed: false,
                    nodes: Vec::new(),
                };
                self.pos = next;
                let quoted = self.code == Some(SINGLE_QUOTE) || self.code == Some(DOUBLE_QUOTE);
                if matches!(function, ValueNode::Function { ref value, .. } if value == "url")
                    && !quoted
                {
                    let mut next = next - 1;
                    loop {
                        let mut escape = false;
                        match find(&self.value, next + 1, CLOSE) {
                            Some(at) => {
                                escape = self.escaped(at);
                                next = at;
                            }
                            None => {
                                self.value.push(CLOSE);
                                next = self.value.len() - 1;
                                if let ValueNode::Function { unclosed, .. } = &mut function {
                                    *unclosed = true;
                                }
                            }
                        }
                        if !escape {
                            break;
                        }
                    }
                    let mut whitespace = next as isize;
                    loop {
                        whitespace -= 1;
                        self.code = if whitespace < 0 {
                            None
                        } else {
                            self.code_at(whitespace as usize)
                        };
                        if !is_space(self.code) {
                            break;
                        }
                    }
                    if let ValueNode::Function {
                        after,
                        nodes,
                        unclosed,
                        ..
                    } = &mut function
                        && (parentheses_open as isize) < whitespace
                    {
                        if self.pos != whitespace as usize + 1 {
                            nodes.push(ValueNode::Word(
                                self.slice(self.pos, whitespace as usize + 1),
                            ));
                        }
                        if *unclosed && whitespace as usize + 1 != next {
                            nodes.push(ValueNode::Space(self.slice(whitespace as usize + 1, next)));
                        } else {
                            *after = self.slice(whitespace as usize + 1, next);
                        }
                    }
                    self.pos = next + 1;
                    self.code = self.code_at(self.pos);
                    self.tokens_mut().push(function);
                } else {
                    self.balanced += 1;
                    self.stack.push(function);
                }
            } else if code == Some(CLOSE) && self.balanced > 0 {
                self.pos = pos + 1;
                self.code = self.code_at(self.pos);
                let mut parent = self.stack.pop().unwrap();
                if let ValueNode::Function { after, .. } = &mut parent {
                    *after = std::mem::take(&mut self.after);
                }
                self.balanced -= 1;
                self.tokens_mut().push(parent);
            } else {
                self.word(pos, max);
            }
        }
        while let Some(mut open) = self.stack.pop() {
            if let ValueNode::Function { unclosed, .. } = &mut open {
                *unclosed = true;
            }
            self.tokens_mut().push(open);
        }
    }

    fn word(&mut self, pos: usize, max: usize) {
        let mut next = pos;
        loop {
            if self.code == Some(BACKSLASH) {
                next += 1;
            }
            next += 1;
            self.code = self.code_at(next);
            let code = self.code;
            let stops = is_space(code)
                || code == Some(SINGLE_QUOTE)
                || code == Some(DOUBLE_QUOTE)
                || code == Some(COMMA)
                || code == Some(COLON)
                || code == Some(SLASH)
                || code == Some(OPEN)
                || (code == Some(STAR) && self.in_calc())
                || (code == Some(CLOSE) && self.balanced > 0);
            if !(next < max && !stops) {
                break;
            }
        }
        let token = self.slice(pos, next);
        if self.code == Some(OPEN) {
            self.name = token;
        } else if starts_with(&token, U_LOWER) || starts_with(&token, U_UPPER) {
            let rest = &token[1..];
            if rest.starts_with('+') && is_unicode_range(&rest[1..]) {
                self.tokens_mut().push(ValueNode::UnicodeRange(token));
            } else {
                self.tokens_mut().push(ValueNode::Word(token));
            }
        } else {
            self.tokens_mut().push(ValueNode::Word(token));
        }
        self.pos = next;
    }
}

fn starts_with(token: &str, unit: u16) -> bool {
    matches!(token.encode_utf16().next(), Some(first) if first == unit)
}

fn find(value: &[u16], from: usize, unit: u16) -> Option<usize> {
    (from..value.len()).find(|at| value[*at] == unit)
}

fn find_pair(value: &[u16], from: usize, first: u16, second: u16) -> Option<usize> {
    (from..value.len().saturating_sub(1)).find(|at| value[*at] == first && value[*at + 1] == second)
}

pub(crate) fn parse(input: &str) -> Vec<ValueNode> {
    let mut parser = ValueParser {
        value: input.encode_utf16().collect(),
        pos: 0,
        code: input.encode_utf16().next(),
        root: Vec::new(),
        stack: Vec::new(),
        balanced: 0,
        name: String::new(),
        before: String::new(),
        after: String::new(),
    };
    parser.run();
    parser.root
}

#[cfg(test)]
mod oracle {
    use serde_json::{Value, json};

    use super::{ValueNode, parse, stringify};

    fn dump(nodes: &[ValueNode]) -> Value {
        Value::Array(nodes.iter().map(dump_node).collect())
    }

    #[allow(clippy::too_many_lines)]
    fn dump_node(node: &ValueNode) -> Value {
        let mut out = serde_json::Map::new();
        let kind = match node {
            ValueNode::Word(_) => "word",
            ValueNode::Space(_) => "space",
            ValueNode::Str { .. } => "string",
            ValueNode::Comment { .. } => "comment",
            ValueNode::Div { .. } => "div",
            ValueNode::UnicodeRange(_) => "unicode-range",
            ValueNode::Function { .. } => "function",
        };
        out.insert("type".into(), json!(kind));
        match node {
            ValueNode::Div {
                value,
                before,
                after,
            } => {
                out.insert("before".into(), json!(before));
                out.insert("after".into(), json!(after));
                out.insert("value".into(), json!(value));
            }
            ValueNode::Str {
                quote,
                value,
                unclosed,
            } => {
                out.insert("quote".into(), json!(quote.to_string()));
                out.insert("unclosed".into(), json!(unclosed));
                out.insert("value".into(), json!(value));
            }
            ValueNode::Comment { value, unclosed } => {
                out.insert("unclosed".into(), json!(unclosed));
                out.insert("value".into(), json!(value));
            }
            ValueNode::Function {
                value,
                before,
                after,
                unclosed,
                nodes,
            } => {
                out.insert("before".into(), json!(before));
                out.insert("after".into(), json!(after));
                out.insert("unclosed".into(), json!(unclosed));
                out.insert("value".into(), json!(value));
                out.insert("nodes".into(), dump(nodes));
            }
            other => {
                out.insert("value".into(), json!(other.value()));
            }
        }
        Value::Object(out)
    }

    fn canon(value: &Value) -> String {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
                keys.sort();
                keys.iter()
                    .map(|key| format!("{}:{}", key, canon(map.get(*key).unwrap())))
                    .collect::<Vec<_>>()
                    .join(",")
            }
            Value::Array(list) => list.iter().map(canon).collect::<Vec<_>>().join(";"),
            other => other.to_string(),
        }
    }

    #[test]
    fn dev_values() {
        let Ok(path) = std::env::var("PG_ORACLE") else {
            return;
        };
        let data: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut failures = 0;
        for case in data["values"].as_array().unwrap() {
            let value = case["value"].as_str().unwrap();
            let nodes = parse(value);
            let mine = json!({ "nodes": dump(&nodes), "stringified": stringify(&nodes) });
            let expected = json!({ "nodes": case["nodes"], "stringified": case["stringified"] });
            if canon(&mine) != canon(&expected) {
                failures += 1;
                if failures <= 15 {
                    println!(
                        "VALUE {value:?}\n  expected {}\n  mine     {}",
                        canon(&expected),
                        canon(&mine)
                    );
                }
            }
        }
        println!("value failures {failures}");
        assert_eq!(failures, 0, "value-parser divergences");
    }
}
