use std::sync::OnceLock;

use postcss_guru::{NodeId, NodeKind, Tree};
use regex::Regex;

use crate::browsers::Browsers;
use crate::data::FEATURE_QUERIES;
use crate::engine::{Flexbox, Prefixes, Session, State};
use crate::utils::{escape_regexp, js_trim};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    Group(Vec<Part>),
}

pub(crate) struct Supports {
    prefixer: OnceLock<Box<Prefixes>>,
}

impl Supports {
    pub(crate) fn new() -> Self {
        Self {
            prefixer: OnceLock::new(),
        }
    }

    fn prefixer(&self, all: &Prefixes) -> &Prefixes {
        self.prefixer.get_or_init(|| {
            let selected: Vec<String> = all
                .browsers
                .selected
                .iter()
                .filter(|browser| FEATURE_QUERIES.contains(&browser.as_str()))
                .cloned()
                .collect();
            Box::new(Prefixes::with_browsers(
                all.options.clone(),
                Browsers::with_selected(selected),
            ))
        })
    }

    pub(crate) fn process(&self, ctx: &mut Session, rule: NodeId) {
        let params = ctx.tree.params(rule).to_string();
        let mut ast = brackets_parse(&params);
        ast = normalize(ast);
        ast = self.remove(ast, &params, ctx);
        ast = self.add(ast, ctx);
        ast = clean_brackets(ast);
        ctx.tree.set_params(rule, brackets_stringify(&ast));
    }

    fn remove(&self, mut nodes: Vec<Part>, all: &str, ctx: &mut Session) -> Vec<Part> {
        static NOT: OnceLock<Regex> = OnceLock::new();
        static OR: OnceLock<Regex> = OnceLock::new();
        let not = NOT.get_or_init(|| Regex::new(r"(?i)not\s*").unwrap());
        let or = OR.get_or_init(|| Regex::new(r"(?i)\s*or\s*").unwrap());
        let mut index = 0;
        while index < nodes.len() {
            let previous_is_not =
                index > 0 && matches!(&nodes[index - 1], Part::Text(text) if not.is_match(text));
            let is_prop = matches!(&nodes[index], Part::Group(group) if group.len() == 1 && matches!(group[0], Part::Text(_)));
            let next_is_or =
                matches!(nodes.get(index + 1), Some(Part::Text(text)) if or.is_match(text));
            if !previous_is_not && is_prop && next_is_or {
                let Part::Group(group) = &nodes[index] else {
                    unreachable!()
                };
                let Part::Text(text) = &group[0] else {
                    unreachable!()
                };
                if self.to_remove(text, all, ctx) {
                    nodes.drain(index..index + 2);
                    continue;
                }
                index += 2;
                continue;
            }
            if let Part::Group(group) = &mut nodes[index] {
                let inner = std::mem::take(group);
                *group = self.remove(inner, all, ctx);
            }
            index += 1;
        }
        nodes
    }

    fn to_remove(&self, text: &str, all: &str, ctx: &Session) -> bool {
        let (prop, value) = parse(text);
        let unprefixed = ctx.prefixes.unprefixed(&prop);
        let cleaner = ctx.prefixes.cleaner();
        if cleaner.remove_decl(&prop).is_some_and(|entry| entry.remove)
            && !is_hack(all, &unprefixed)
        {
            return true;
        }
        cleaner
            .remove_values(&unprefixed)
            .iter()
            .any(|checker| checker.check(&value))
    }

    fn add(&self, nodes: Vec<Part>, ctx: &mut Session) -> Vec<Part> {
        nodes
            .into_iter()
            .map(|part| match part {
                Part::Group(group) if group.len() == 1 && matches!(group[0], Part::Text(_)) => {
                    let Part::Text(text) = &group[0] else {
                        unreachable!()
                    };
                    let prefixed = self.prefixed(text, ctx);
                    if prefixed.len() > 1 {
                        return convert(&prefixed);
                    }
                    Part::Group(group)
                }
                Part::Group(group) => Part::Group(self.add(group, ctx)),
                other => other,
            })
            .collect()
    }

    fn prefixed(&self, text: &str, ctx: &Session) -> Vec<(String, String)> {
        let all = ctx.prefixes;
        let (prop, value) = parse(text);
        let mut tree = Tree::new(None);
        let rule = tree.create(NodeKind::Rule, "a");
        tree.append(tree.root, rule);
        let decl = tree.create(NodeKind::Decl, prop.clone());
        tree.set_value(decl, value);
        tree.raws_mut(decl).before = Some(String::new());
        tree.append(rule, decl);
        if disabled(&tree, decl, all) {
            return nodes_of(&tree, rule);
        }
        let nested = self.prefixer(all);
        let mut state = State::new(nested);
        if let Some(prefixer) = nested.add_decl(&prop) {
            prefixer
                .hack
                .process(prefixer, &mut Session::new(&mut tree, &mut state), decl);
        }
        let mut index = 0;
        while index < tree.child_count(rule) {
            let decl = tree.children(rule)[index];
            let first_prop = tree
                .first(rule)
                .map(|first| tree.prop(first).to_string())
                .unwrap_or_default();
            for value in nested.add_values(&first_prop) {
                value.process(&mut Session::new(&mut tree, &mut state), decl);
            }
            let mut outer = State::new(all);
            crate::values::save(all, &mut Session::new(&mut tree, &mut outer), decl);
            index += 1;
        }
        nodes_of(&tree, rule)
    }
}

fn nodes_of(tree: &Tree, rule: postcss_guru::NodeId) -> Vec<(String, String)> {
    tree.children(rule)
        .iter()
        .map(|decl| (tree.prop(*decl).to_string(), tree.value(*decl).to_string()))
        .collect()
}

fn convert(progress: &[(String, String)]) -> Part {
    let mut result: Vec<Part> = vec![Part::Text(String::new())];
    for (prop, value) in progress {
        result.push(Part::Group(vec![Part::Text(format!("{prop}: {value}"))]));
        result.push(Part::Text(String::from(" or ")));
    }
    if let Some(last) = result.last_mut() {
        *last = Part::Text(String::new());
    }
    Part::Group(result)
}

fn clean_brackets(nodes: Vec<Part>) -> Vec<Part> {
    nodes
        .into_iter()
        .map(|part| match part {
            Part::Group(group) if group.len() == 1 && matches!(group[0], Part::Group(_)) => {
                let Part::Group(inner) = group.into_iter().next().unwrap() else {
                    unreachable!()
                };
                Part::Group(clean_brackets(inner))
            }
            Part::Group(group) => Part::Group(clean_brackets(group)),
            other => other,
        })
        .collect()
}

fn disabled(tree: &Tree, node: postcss_guru::NodeId, all: &Prefixes) -> bool {
    let prop = tree.prop(node);
    let value = tree.value(node);
    all.options.grid_disabled()
        && (prop == "display" && value.contains("grid")
            || prop.contains("grid")
            || prop == "justify-items")
        || all.options.flexbox == Flexbox::None
            && (prop == "display" && value.contains("flex")
                || prop.contains("flex")
                || ["order", "justify-content", "align-items", "align-content"].contains(&prop))
}

fn is_hack(all: &str, unprefixed: &str) -> bool {
    !Regex::new(&format!(r"(\(|\s){}:", escape_regexp(unprefixed)))
        .unwrap()
        .is_match(all)
}

fn parse(text: &str) -> (String, String) {
    let mut parts = text.splitn(2, ':');
    let prop = js_trim(parts.next().unwrap_or(""));
    let value = js_trim(parts.next().unwrap_or(""));
    (prop, value)
}

fn normalize(nodes: Vec<Part>) -> Vec<Part> {
    let filtered: Vec<Part> = nodes
        .into_iter()
        .filter(|part| !matches!(part, Part::Text(text) if text.is_empty()))
        .collect();
    if let Some(Part::Text(first)) = filtered.first() {
        let first = js_trim(first);
        if first.contains(':') || first == "selector" || first == "not selector" {
            return vec![Part::Text(brackets_stringify(&filtered))];
        }
    }
    filtered
        .into_iter()
        .map(|part| match part {
            Part::Group(group) => Part::Group(normalize(group)),
            other => other,
        })
        .collect()
}

fn brackets_parse(text: &str) -> Vec<Part> {
    let mut root = vec![Part::Text(String::new())];
    let mut open: Vec<Vec<Part>> = Vec::new();
    for symbol in text.chars() {
        if symbol == '(' {
            open.push(vec![Part::Text(String::new())]);
            match open.last_mut() {
                Some(_) => {
                    let parent = if open.len() == 1 {
                        &mut root
                    } else {
                        let depth = open.len() - 2;
                        &mut open[depth]
                    };
                    parent.push(Part::Group(Vec::new()));
                }
                None => unreachable!(),
            }
        } else if symbol == ')' {
            if let Some(group) = open.pop() {
                let parent = open.last_mut().unwrap_or(&mut root);
                if let Some(last @ Part::Group(_)) = parent.last_mut() {
                    *last = Part::Group(group);
                }
                parent.push(Part::Text(String::new()));
            }
        } else if let Some(Part::Text(last)) = open.last_mut().unwrap_or(&mut root).last_mut() {
            last.push(symbol);
        }
    }
    root
}

fn brackets_stringify(parts: &[Part]) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            Part::Group(group) => {
                out.push('(');
                out.push_str(&brackets_stringify(group));
                out.push(')');
            }
            Part::Text(text) => out.push_str(text),
        }
    }
    out
}
