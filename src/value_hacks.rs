use std::sync::OnceLock;

use postcss_guru::NodeId;
use regex::Regex;

use crate::declaration::{Spec, flex_spec};
use crate::engine::Session;
use crate::utils;
use crate::value::{self, ValueNode};
use crate::values::{OldValue, Value, ValueHack};

struct DisplayFlex;

impl ValueHack for DisplayFlex {
    fn name(&self, name: &str) -> String {
        if name == "display-flex" {
            String::from("flex")
        } else {
            name.to_string()
        }
    }

    fn check(&self, value: &Value, ctx: &Session, node: NodeId) -> bool {
        ctx.tree.prop(node) == "display" && ctx.tree.value(node) == value.name
    }

    fn old(&self, value: &Value, prefix: &str) -> Option<OldValue> {
        let prefixed = self.prefixed(value, prefix);
        Some(OldValue::new(value.name.clone(), prefixed, None, None))
    }

    fn replace(&self, value: &Value, _string: &str, prefix: &str) -> Option<String> {
        Some(self.prefixed(value, prefix))
    }
}

impl DisplayFlex {
    fn prefixed(&self, value: &Value, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        let name = match spec {
            Spec::V2009 => {
                if value.name == "flex" {
                    "box"
                } else {
                    "inline-box"
                }
            }
            Spec::V2012 => {
                if value.name == "flex" {
                    "flexbox"
                } else {
                    "inline-flexbox"
                }
            }
            Spec::Final => return format!("{prefix}{}", value.name),
            Spec::Unknown => "undefined",
        };
        format!("{prefix}{name}")
    }
}

struct DisplayGrid;

impl ValueHack for DisplayGrid {
    fn name(&self, name: &str) -> String {
        if name == "display-grid" {
            String::from("grid")
        } else {
            name.to_string()
        }
    }

    fn check(&self, value: &Value, ctx: &Session, node: NodeId) -> bool {
        ctx.tree.prop(node) == "display" && ctx.tree.value(node) == value.name
    }
}

struct FilterValue;

impl ValueHack for FilterValue {
    fn name(&self, name: &str) -> String {
        if name == "filter-function" {
            String::from("filter")
        } else {
            name.to_string()
        }
    }
}

struct ImageSet;

impl ValueHack for ImageSet {
    fn replace(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        let fixed = crate::values::BASE.replace(value, string, prefix)?;
        if prefix == "-webkit-" {
            static QUOTED: OnceLock<Regex> = OnceLock::new();
            let regex = QUOTED.get_or_init(|| {
                Regex::new("(?i)(\"[^\"]+\"|'[^']+')([ \\t\\n\\x0B\\x0C\\r]+\\d+[a-zA-Z])").unwrap()
            });
            return Some(regex.replace(&fixed, "url($1)$2").into_owned());
        }
        Some(fixed)
    }
}

struct Pixelated;

impl ValueHack for Pixelated {
    fn old(&self, value: &Value, prefix: &str) -> Option<OldValue> {
        match prefix {
            "-webkit-" => Some(OldValue::new(
                value.name.clone(),
                String::from("-webkit-optimize-contrast"),
                None,
                None,
            )),
            "-moz-" => Some(OldValue::new(
                value.name.clone(),
                String::from("-moz-crisp-edges"),
                None,
                None,
            )),
            _ => crate::values::BASE.old(value, prefix),
        }
    }

    fn replace(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        let replacement = match prefix {
            "-webkit-" => "$1-webkit-optimize-contrast",
            "-moz-" => "$1-moz-crisp-edges",
            _ => return crate::values::BASE.replace(value, string, prefix),
        };
        Some(
            utils::regexp(&value.name, true)
                .replace_all(string, replacement)
                .into_owned(),
        )
    }
}

struct CrossFade;

impl ValueHack for CrossFade {
    fn replace(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        static NUMBER: OnceLock<Regex> = OnceLock::new();
        let parts: Vec<String> = utils::space(string)
            .into_iter()
            .map(|part| {
                if !part.starts_with(&format!("{}(", value.name)) {
                    return part;
                }
                let Some(close) = part.rfind(')') else {
                    return part;
                };
                let after = part[close + 1..].to_string();
                let mut args = part[value.name.len() + 1..close].to_string();
                if prefix == "-webkit-" {
                    let number = NUMBER.get_or_init(|| Regex::new(r"\d*.?\d+%?").unwrap());
                    match number.find(&args).map(|found| found.as_str().to_string()) {
                        Some(found) => {
                            args = utils::js_trim(&args[found.len()..]);
                            args.push_str(&format!(", {found}"));
                        }
                        None => args.push_str(", 0.5"),
                    }
                }
                format!("{prefix}{}({args}){after}", value.name)
            })
            .collect();
        Some(parts.join(" "))
    }
}

struct Intrinsic;

impl Intrinsic {
    fn is_stretch(&self, value: &Value) -> bool {
        matches!(value.name.as_str(), "stretch" | "fill" | "fill-available")
    }

    fn regexp(name: &str) -> Regex {
        Regex::new(&format!(
            r"(?i)(^|[{}\(,])({}($|[{}\),]))",
            utils::JS_SPACE,
            utils::escape_regexp(name),
            utils::JS_SPACE
        ))
        .unwrap()
    }
}

impl ValueHack for Intrinsic {
    fn add(&self, value: &Value, ctx: &mut Session, node: NodeId, prefix: &str) {
        if ctx.tree.prop(node).contains("grid") && prefix != "-webkit-" {
            return;
        }
        crate::values::base_add(self, value, ctx, node, prefix);
    }

    fn old(&self, value: &Value, prefix: &str) -> Option<OldValue> {
        let mut prefixed = format!("{prefix}{}", value.name);
        if self.is_stretch(value) {
            if prefix == "-moz-" {
                prefixed = String::from("-moz-available");
            } else if prefix == "-webkit-" {
                prefixed = String::from("-webkit-fill-available");
            }
        }
        let regexp = Self::regexp(&prefixed);
        Some(OldValue::new(
            value.name.clone(),
            prefixed.clone(),
            Some(prefixed),
            Some(regexp),
        ))
    }

    fn replace(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        let replacement = match prefix {
            "-moz-" if self.is_stretch(value) => "$1-moz-available$3",
            "-webkit-" if self.is_stretch(value) => "$1-webkit-fill-available$3",
            _ => return crate::values::BASE.replace(value, string, prefix),
        };
        Some(
            Self::regexp(&value.name)
                .replace_all(string, replacement)
                .into_owned(),
        )
    }
}

const DIRECTIONS: &[(&str, &str)] = &[
    ("bottom", "top"),
    ("left", "right"),
    ("right", "left"),
    ("top", "bottom"),
];

const OLD_DIRECTIONS: &[(&str, &str)] = &[
    ("bottom", "left top, left bottom"),
    ("bottom left", "right top, left bottom"),
    ("bottom right", "left top, right bottom"),
    ("left", "right top, left top"),
    ("left bottom", "right top, left bottom"),
    ("left top", "right bottom, left top"),
    ("right", "left top, right top"),
    ("right bottom", "left top, right bottom"),
    ("right top", "left bottom, right top"),
    ("top", "left bottom, left top"),
    ("top left", "right bottom, left top"),
    ("top right", "left bottom, right top"),
];

struct Gradient;

impl Gradient {
    fn clone_div(params: &[Vec<ValueNode>]) -> ValueNode {
        for param in params {
            for node in param {
                if let ValueNode::Div { value, after, .. } = node
                    && value == ","
                {
                    return ValueNode::Div {
                        value: value.clone(),
                        before: String::new(),
                        after: after.clone(),
                    };
                }
            }
        }
        ValueNode::Div {
            value: String::from(","),
            before: String::new(),
            after: String::from(" "),
        }
    }

    fn revert_direction(&self, word: &str) -> String {
        let lower = word.to_lowercase();
        DIRECTIONS
            .iter()
            .find(|(from, _)| *from == lower)
            .map_or_else(|| word.to_string(), |(_, to)| to.to_string())
    }

    fn new_direction(&self, params: &mut Vec<ValueNode>) {
        let Some(first) = params.first() else {
            return;
        };
        let first = first.value().to_string();
        if first == "to" || !is_direction(&first) {
            return;
        }
        params.insert(0, ValueNode::Word(String::from("to")));
        params.insert(1, ValueNode::Space(String::from(" ")));
        for node in params.iter_mut().skip(2) {
            match node {
                ValueNode::Div { .. } => break,
                ValueNode::Word(word) => *word = self.revert_direction(word),
                _ => {}
            }
        }
    }

    fn normalize(&self, nodes: &mut Vec<ValueNode>, gradient: &str) {
        static GRAD: OnceLock<Regex> = OnceLock::new();
        static RAD: OnceLock<Regex> = OnceLock::new();
        static TURN: OnceLock<Regex> = OnceLock::new();
        let Some(first) = nodes.first_mut() else {
            return;
        };
        let value = first.value().to_string();
        if GRAD
            .get_or_init(|| Regex::new(r"-?\d+(.\d+)?grad").unwrap())
            .is_match(&value)
        {
            first.set_value(self.normalize_unit(&value, 400.0));
        } else if RAD
            .get_or_init(|| Regex::new(r"-?\d+(.\d+)?rad").unwrap())
            .is_match(&value)
        {
            first.set_value(self.normalize_unit(&value, 2.0 * std::f64::consts::PI));
        } else if TURN
            .get_or_init(|| Regex::new(r"-?\d+(.\d+)?turn").unwrap())
            .is_match(&value)
        {
            first.set_value(self.normalize_unit(&value, 1.0));
        } else if value.contains("deg") {
            let num = utils::parse_float(&value);
            let num = (num % 360.0 + 360.0) % 360.0;
            first.set_value(format!("{}deg", utils::number(num)));
        }
        if gradient == "linear-gradient" || gradient == "repeating-linear-gradient" {
            let direction = first.value().to_string();
            let words: &[&str] = match direction.as_str() {
                "0deg" | "0" => &["to", " ", "top"],
                "90deg" => &["to", " ", "right"],
                "180deg" => &["to", " ", "bottom"],
                "270deg" => &["to", " ", "left"],
                _ => return,
            };
            *nodes = self.replace_first(nodes, words);
        }
    }

    fn normalize_unit(&self, text: &str, full: f64) -> String {
        let num = utils::parse_float(text);
        let deg = (num / full) * 360.0;
        format!("{}deg", utils::number(deg))
    }

    fn replace_first(&self, params: &[ValueNode], words: &[&str]) -> Vec<ValueNode> {
        let mut prefix: Vec<ValueNode> = words
            .iter()
            .map(|word| {
                if *word == " " {
                    ValueNode::Space(String::from(" "))
                } else {
                    ValueNode::Word(word.to_string())
                }
            })
            .collect();
        prefix.extend_from_slice(&params[1..]);
        prefix
    }

    fn convert_direction(&self, params: &mut Vec<ValueNode>) {
        if let Some(first) = params.first() {
            let first = first.value().to_string();
            if first == "to" {
                self.fix_direction(params);
            } else if first.contains("deg") {
                self.fix_angle(params);
            } else if self.is_radial(params) {
                self.fix_radial(params);
            }
        }
    }

    fn fix_angle(&self, params: &mut [ValueNode]) {
        let first = utils::parse_float(params[0].value());
        let first = (450.0 - first).abs() % 360.0;
        let first = round_float(first, 3);
        params[0].set_value(format!("{first}deg"));
    }

    fn fix_direction(&self, params: &mut Vec<ValueNode>) {
        params.drain(..2);
        for node in params.iter_mut() {
            match node {
                ValueNode::Div { .. } => break,
                ValueNode::Word(word) => *word = self.revert_direction(word),
                _ => {}
            }
        }
    }

    fn is_radial(&self, params: &[ValueNode]) -> bool {
        let mut state = "before";
        for param in params {
            if state == "before" && matches!(param, ValueNode::Space(_)) {
                state = "at";
            } else if state == "at" && param.value() == "at" {
                state = "after";
            } else if state == "after" && matches!(param, ValueNode::Space(_)) {
                return true;
            } else if matches!(param, ValueNode::Div { .. }) {
                break;
            } else {
                state = "before";
            }
        }
        false
    }

    fn fix_radial(&self, params: &mut Vec<ValueNode>) -> Option<()> {
        let mut first = Vec::new();
        let mut second = Vec::new();
        let mut next = None;
        for i in 0..params.len().saturating_sub(2) {
            let a = &params[i];
            let b = &params[i + 1];
            let c = &params[i + 2];
            if matches!(a, ValueNode::Space(_))
                && b.value() == "at"
                && matches!(c, ValueNode::Space(_))
            {
                next = Some(i + 3);
                break;
            }
            first.push(params[i].clone());
        }
        let next = next?;
        let mut div = None;
        let mut i = next;
        while i < params.len() {
            if matches!(params[i], ValueNode::Div { .. }) {
                div = Some(params[i].clone());
                break;
            }
            second.push(params[i].clone());
            i += 1;
        }
        let div = div?;
        let mut replacement = second;
        replacement.push(div);
        replacement.extend(first);
        params.splice(0..i, replacement);
        Some(())
    }

    fn old_direction(&self, params: &mut Vec<Vec<ValueNode>>) {
        let div = Self::clone_div(params);
        if params[0][0].value() != "to" {
            params.insert(
                0,
                vec![ValueNode::Word(String::from("left top, left bottom")), div],
            );
        } else {
            let words: Vec<String> = params[0][2..]
                .iter()
                .filter(|node| matches!(node, ValueNode::Word(_)))
                .map(|node| node.value().to_lowercase())
                .collect();
            let words = words.join(" ");
            let old = OLD_DIRECTIONS
                .iter()
                .find(|(from, _)| *from == words)
                .map_or(words.as_str(), |(_, to)| *to);
            params[0] = vec![ValueNode::Word(old.to_string()), div];
        }
    }

    fn old_webkit(&self, node: &mut ValueNode) -> bool {
        let ValueNode::Function { value, nodes, .. } = node else {
            return false;
        };
        let string = value::stringify(nodes);
        if value != "linear-gradient" {
            return false;
        }
        if nodes
            .first()
            .is_some_and(|first| first.value().contains("deg"))
        {
            return false;
        }
        if string.contains("px") || string.contains("-corner") || string.contains("-side") {
            return false;
        }
        let mut params: Vec<Vec<ValueNode>> = vec![Vec::new()];
        for entry in nodes.iter() {
            let last = params.last_mut().expect("params are not empty");
            last.push(entry.clone());
            if matches!(entry, ValueNode::Div { value, .. } if value == ",") {
                params.push(Vec::new());
            }
        }
        self.old_direction(&mut params);
        Self::color_stops_static(&mut params);
        let mut flat: Vec<ValueNode> = Vec::new();
        for param in &params {
            flat.extend(param.iter().cloned());
        }
        flat.insert(0, Self::clone_div(&params));
        flat.insert(0, ValueNode::Word(String::from("linear")));
        *nodes = flat;
        *value = String::from("-webkit-gradient");
        true
    }

    fn color_stops_static(params: &mut [Vec<ValueNode>]) {
        let count = params.len();
        for (i, param) in params.iter_mut().enumerate().skip(1) {
            let color = value::stringify_node(&param[0]);
            let pos = param
                .get(1)
                .filter(|node| matches!(node, ValueNode::Word(_)))
                .or_else(|| {
                    param
                        .get(2)
                        .filter(|node| matches!(node, ValueNode::Word(_)))
                })
                .map(|node| node.value().to_string());
            let stop = if i == 1 && pos.as_deref().is_none_or(|pos| pos == "0%") {
                format!("from({color})")
            } else if i == count - 1 && pos.as_deref().is_none_or(|pos| pos == "100%") {
                format!("to({color})")
            } else if let Some(pos) = pos {
                format!("color-stop({pos}, {color})")
            } else {
                format!("color-stop({color})")
            };
            let div = param.last().cloned();
            *param = vec![ValueNode::Word(stop)];
            if let Some(node) = div
                && matches!(&node, ValueNode::Div { value, .. } if value.as_str() == ",")
            {
                param.push(node);
            }
        }
    }
}

fn is_direction(word: &str) -> bool {
    static DIRECTION: OnceLock<Regex> = OnceLock::new();
    DIRECTION
        .get_or_init(|| Regex::new("(?i)top|left|right|bottom").unwrap())
        .is_match(word)
}

fn round_float(float: f64, digits: usize) -> f64 {
    let text = format!("{float:.digits$}");
    utils::parse_float(&text)
}

impl ValueHack for Gradient {
    fn add(&self, value: &Value, ctx: &mut Session, node: NodeId, prefix: &str) {
        let prop = ctx.tree.prop(node);
        let restricted =
            prop.contains("mask") || matches!(prop, "list-style" | "list-style-image" | "content");
        let allowed = !restricted || prefix == "-webkit-" || prefix == "-webkit- old";
        if allowed {
            crate::values::base_add(self, value, ctx, node, prefix);
        }
    }

    fn old(&self, value: &Value, prefix: &str) -> Option<OldValue> {
        if prefix != "-webkit-" {
            return crate::values::BASE.old(value, prefix);
        }
        let kind = match value.name.as_str() {
            "linear-gradient" => "linear",
            "repeating-linear-gradient" => "repeating-linear",
            "repeating-radial-gradient" => "repeating-radial",
            _ => "radial",
        };
        let regexp = utils::regexp(
            &format!("-webkit-({kind}-gradient|gradient\\(\\s*{kind})"),
            false,
        );
        Some(OldValue::new(
            value.name.clone(),
            format!("-webkit-{}", value.name),
            Some(String::from("-gradient")),
            Some(regexp),
        ))
    }

    fn replace(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        self.replace_gradient(value, string, prefix)
    }
}

impl Gradient {
    fn replace_gradient(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        let mut nodes = value::parse(string);
        for node in &mut nodes {
            if let ValueNode::Function {
                value: name, nodes, ..
            } = node
                && name == &value.name
            {
                let mut params = std::mem::take(nodes);
                self.new_direction(&mut params);
                self.normalize(&mut params, &value.name);
                if prefix == "-webkit- old" {
                    let mut function = ValueNode::Function {
                        value: name.clone(),
                        before: String::new(),
                        after: String::new(),
                        unclosed: false,
                        nodes: params,
                    };
                    if !self.old_webkit(&mut function) {
                        return None;
                    }
                    let ValueNode::Function {
                        value: new_name,
                        nodes: new_nodes,
                        ..
                    } = function
                    else {
                        unreachable!()
                    };
                    *name = new_name;
                    *nodes = new_nodes;
                } else {
                    self.convert_direction(&mut params);
                    *nodes = params;
                    *name = format!("{prefix}{name}");
                }
            }
        }
        Some(value::stringify(&nodes))
    }
}

pub(crate) static VALUE_HACKS: &[(&[&str], &dyn ValueHack)] = &[
    (
        &[
            "linear-gradient",
            "repeating-linear-gradient",
            "radial-gradient",
            "repeating-radial-gradient",
        ],
        &Gradient,
    ),
    (
        &[
            "max-content",
            "min-content",
            "fit-content",
            "fill",
            "fill-available",
            "stretch",
        ],
        &Intrinsic,
    ),
    (&["pixelated"], &Pixelated),
    (&["image-set"], &ImageSet),
    (&["cross-fade"], &CrossFade),
    (&["display-flex", "inline-flex"], &DisplayFlex),
    (&["display-grid", "inline-grid"], &DisplayGrid),
    (&["filter", "filter-function"], &FilterValue),
];
