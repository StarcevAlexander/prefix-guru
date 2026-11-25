use postcss_guru::{NodeId, NodeKind};
use regex::Regex;

use crate::engine::{self, Session};
use crate::utils::collapse_first_spaces;
use crate::vendor;

#[derive(Clone)]
pub(crate) struct Value {
    pub name: String,
    pub prefixes: Vec<String>,
    pub hack: &'static dyn ValueHack,
}

impl Value {
    pub(crate) fn load(name: &str, prefixes: Vec<String>) -> Self {
        let hack = engine::value_hack(name);
        Self {
            name: hack.name(name),
            prefixes,
            hack,
        }
    }

    pub(crate) fn process(&self, ctx: &mut Session, node: NodeId) {
        engine_process_value(self, ctx, node);
    }

    pub(crate) fn old(&self, prefix: &str) -> Option<OldValue> {
        self.hack.old(self, prefix)
    }
}

#[derive(Clone)]
pub(crate) struct OldValue {
    pub unprefixed: String,
    pub string: String,
    pub regexp: Regex,
}

impl OldValue {
    pub(crate) fn new(
        unprefixed: String,
        prefixed: String,
        string: Option<String>,
        regexp: Option<Regex>,
    ) -> Self {
        let string = string.unwrap_or_else(|| prefixed.clone());
        let regexp = regexp.unwrap_or_else(|| crate::utils::regexp(&prefixed, true));
        Self {
            unprefixed,
            string,
            regexp,
        }
    }

    pub(crate) fn check(&self, value: &str) -> bool {
        value.contains(&self.string) && self.regexp.is_match(value)
    }
}

pub(crate) trait ValueHack: Sync {
    fn name(&self, name: &str) -> String {
        name.to_string()
    }

    fn add(&self, value: &Value, ctx: &mut Session, node: NodeId, prefix: &str) {
        base_add(self, value, ctx, node, prefix);
    }

    fn check(&self, value: &Value, ctx: &Session, node: NodeId) -> bool {
        let text = ctx.tree.value(node);
        text.contains(value.name.as_str()) && crate::utils::regexp(&value.name, true).is_match(text)
    }

    fn old(&self, value: &Value, prefix: &str) -> Option<OldValue> {
        Some(OldValue::new(
            value.name.clone(),
            format!("{prefix}{}", value.name),
            None,
            None,
        ))
    }

    fn replace(&self, value: &Value, string: &str, prefix: &str) -> Option<String> {
        Some(
            crate::utils::regexp(&value.name, true)
                .replace_all(string, format!("${{1}}{prefix}$2"))
                .into_owned(),
        )
    }

    fn value(&self, _value: &Value, ctx: &Session, node: NodeId) -> String {
        raw_value(ctx, node)
    }
}

pub(crate) struct Base;

impl ValueHack for Base {}

pub(crate) static BASE: Base = Base;

fn raw_value(ctx: &Session, node: NodeId) -> String {
    match ctx.tree.raw_text(node) {
        Some(raw) if raw.value == ctx.tree.value(node) => raw.raw.clone(),
        _ => ctx.tree.value(node).to_string(),
    }
}

pub(crate) fn base_add<T: ValueHack + ?Sized>(
    hack: &T,
    value: &Value,
    ctx: &mut Session,
    node: NodeId,
    prefix: &str,
) {
    let cached = ctx
        .cached(node)
        .values
        .unwrap_or_default()
        .into_iter()
        .find(|(at, _)| at == prefix)
        .map_or_else(|| hack.value(value, ctx, node), |(_, text)| text);
    let mut text = cached;
    loop {
        let before = text.clone();
        let Some(replaced) = hack.replace(value, &text, prefix) else {
            return;
        };
        text = replaced;
        if text == before {
            break;
        }
    }
    let cache = ctx.cache(node);
    let values = cache.values.get_or_insert_with(Vec::new);
    match values.iter_mut().find(|(at, _)| at == prefix) {
        Some((_, stored)) => *stored = text,
        None => values.push((prefix.to_string(), text)),
    }
}

fn engine_process_value(value: &Value, ctx: &mut Session, node: NodeId) {
    if !value.hack.check(value, ctx, node) {
        return;
    }
    let parent = ctx.parent_prefix(node);
    let prefixes: Vec<String> = value
        .prefixes
        .iter()
        .filter(|prefix| {
            parent
                .as_ref()
                .is_none_or(|parent| parent == crate::utils::remove_note(prefix))
        })
        .cloned()
        .collect();
    for prefix in prefixes {
        value.hack.add(value, ctx, node, &prefix);
    }
}

pub(crate) fn save(prefixes: &engine::Prefixes, ctx: &mut Session, decl: NodeId) {
    let prop = ctx.tree.prop(decl).to_string();
    let Some(values) = ctx.cached(decl).values else {
        return;
    };
    for (prefix, value) in values {
        if value == ctx.tree.value(decl) {
            continue;
        }
        let prop_prefix = vendor::prefix(&prop);
        if prop_prefix == "-pie-" {
            continue;
        }
        if prop_prefix == prefix {
            ctx.tree.set_value(decl, value);
            continue;
        }
        let prefixed = prefixes.prefixed(&prop, &prefix);
        if ctx.parent_has(decl, |tree, child| {
            tree.is(child, NodeKind::Decl) && tree.prop(child) == prefixed
        }) {
            continue;
        }
        let trimmed = collapse_first_spaces(&value);
        let already = ctx.parent_has(decl, |tree, child| {
            tree.is(child, NodeKind::Decl)
                && tree.prop(child) == prop
                && collapse_first_spaces(tree.value(child)) == trimmed
        });
        if already {
            continue;
        }
        let cloned = ctx.clone(decl);
        ctx.tree.set_value(cloned, value);
        ctx.tree.insert_before(decl, cloned);
    }
}
