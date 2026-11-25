use postcss_guru::{NodeId, NodeKind};

use crate::engine::Session;
use crate::utils::comma;
use crate::value::{self, ValueNode};
use crate::vendor;

pub(crate) struct Transition;

impl Transition {
    pub(crate) fn add(&self, ctx: &mut Session, decl: NodeId) {
        let prop = ctx.tree.prop(decl).to_string();
        let text = ctx.tree.value(decl).to_string();
        let add = ctx.prefixes.add_decl(&prop);
        let vendor_prefixes = rule_vendor_prefixes(ctx, decl);
        let decl_prefixes: Vec<String> = vendor_prefixes
            .clone()
            .unwrap_or_else(|| add.map_or_else(Vec::new, |add| add.prefixes.clone()));
        let mut params = parse(&text);
        let names: Vec<String> = params.iter().map(|param| find_prop(param)).collect();
        let mut added: Vec<Vec<ValueNode>> = Vec::new();
        if names.iter().any(|name| name.starts_with('-')) {
            return;
        }
        for param in &params {
            let prop = find_prop(param);
            if prop.starts_with('-') {
                continue;
            }
            let Some(prefixer) = ctx.prefixes.add_decl(&prop) else {
                continue;
            };
            for prefix in prefixer.prefixes.iter().map(String::as_str) {
                if vendor_prefixes.as_ref().is_some_and(|vendors| {
                    !vendors
                        .iter()
                        .any(|vendor| prefix.contains(vendor.as_str()))
                }) {
                    continue;
                }
                let prefixed = ctx.prefixes.prefixed(&prop, prefix);
                if prefixed != "-ms-transform"
                    && !names.contains(&prefixed)
                    && !disabled(ctx, &prop, prefix)
                {
                    added.push(clone(param, &prop, &prefixed));
                }
            }
        }
        params.extend(added);
        let value = stringify(&mut params);
        let mut webkit = clean_from_unprefixed(ctx, &params, "-webkit-");
        let webkit_clean = stringify(&mut webkit);
        if decl_prefixes.iter().any(|prefix| prefix == "-webkit-") {
            clone_before(ctx, decl, &format!("-webkit-{prop}"), &webkit_clean);
        }
        clone_before(ctx, decl, &prop, &webkit_clean);
        if decl_prefixes.iter().any(|prefix| prefix == "-o-") {
            let mut opera = clean_from_unprefixed(ctx, &params, "-o-");
            let opera_clean = stringify(&mut opera);
            clone_before(ctx, decl, &format!("-o-{prop}"), &opera_clean);
        }
        for prefix in decl_prefixes
            .iter()
            .filter(|prefix| *prefix != "-webkit-" && *prefix != "-o-")
        {
            let mut others = clean_other_prefixes(&params, prefix);
            let text = stringify(&mut others);
            clone_before(ctx, decl, &format!("{prefix}{prop}"), &text);
        }
        if value != ctx.tree.value(decl) && !already(ctx, decl, &prop, &value) {
            check_for_warning(ctx, decl);
            let copy = ctx.clone_full(decl);
            ctx.tree.insert_before(decl, copy);
            ctx.tree.set_value(decl, value);
        }
    }

    pub(crate) fn remove(&self, ctx: &mut Session, decl: NodeId) {
        let text = ctx.tree.value(decl).to_string();
        let mut params: Vec<Vec<ValueNode>> = parse(&text)
            .into_iter()
            .filter(|param| {
                let prop = find_prop(param);
                ctx.prefixes
                    .remove_decl(&prop)
                    .is_none_or(|entry| !entry.remove)
            })
            .collect();
        let value = stringify(&mut params);
        if ctx.tree.value(decl) == value {
            return;
        }
        if params.is_empty() {
            ctx.tree.remove(decl);
            return;
        }
        let prop = ctx.tree.prop(decl).to_string();
        let double = ctx.parent_has(decl, |tree, child| {
            tree.is(child, NodeKind::Decl) && tree.prop(child) == prop && tree.value(child) == value
        });
        let smaller = ctx.parent_has(decl, |tree, child| {
            child != decl
                && tree.is(child, NodeKind::Decl)
                && tree.prop(child) == prop
                && tree.value(child).encode_utf16().count() > value.encode_utf16().count()
        });
        if double || smaller {
            ctx.tree.remove(decl);
            return;
        }
        ctx.tree.set_value(decl, value);
    }
}

fn clone_before(ctx: &mut Session, decl: NodeId, prop: &str, value: &str) {
    if already(ctx, decl, prop, value) {
        return;
    }
    let cloned = ctx.clone_full(decl);
    ctx.tree.set_prop(cloned, prop);
    ctx.tree.set_value(cloned, value);
    ctx.tree.insert_before(decl, cloned);
}

fn already(ctx: &Session, decl: NodeId, prop: &str, value: &str) -> bool {
    ctx.parent_has(decl, |tree, child| {
        tree.is(child, NodeKind::Decl) && tree.prop(child) == prop && tree.value(child) == value
    })
}

fn check_for_warning(ctx: &mut Session, decl: NodeId) {
    if ctx.tree.prop(decl) != "transition-property" {
        return;
    }
    let mut is_prefixed = false;
    let mut has_associated = false;
    if let Some(parent) = ctx.tree.parent(decl) {
        for child in ctx.tree.children(parent).to_vec() {
            if !ctx.tree.is(child, NodeKind::Decl)
                || !ctx.tree.prop(child).starts_with("transition-")
            {
                continue;
            }
            let values = comma(ctx.tree.value(child));
            if ctx.tree.prop(child) == "transition-property" {
                for value in values {
                    if let Some(lookup) = ctx.prefixes.add_decl(&value)
                        && !lookup.prefixes.is_empty()
                    {
                        is_prefixed = true;
                    }
                }
                continue;
            }
            has_associated |= values.len() > 1;
            break;
        }
    }
    if is_prefixed && has_associated {
        ctx.warn(
            decl,
            "Replace transition-property to transition, because Autoprefixer could not support any cases of transition-property and other transition-*",
        );
    }
}

fn clean_from_unprefixed(
    ctx: &Session,
    params: &[Vec<ValueNode>],
    prefix: &str,
) -> Vec<Vec<ValueNode>> {
    let remove: Vec<String> = params
        .iter()
        .map(|param| find_prop(param))
        .filter(|prop| prop.starts_with(prefix))
        .map(|prop| ctx.prefixes.unprefixed(&prop))
        .collect();
    params
        .iter()
        .filter(|param| {
            let prop = find_prop(param);
            let vendor = vendor::prefix(&prop);
            !remove.contains(&prop) && (vendor == prefix || vendor.is_empty())
        })
        .cloned()
        .collect()
}

fn clean_other_prefixes(params: &[Vec<ValueNode>], prefix: &str) -> Vec<Vec<ValueNode>> {
    params
        .iter()
        .filter(|param| {
            let current = vendor::prefix(&find_prop(param));
            current.is_empty() || current == prefix
        })
        .cloned()
        .collect()
}

fn clone(param: &[ValueNode], origin: &str, name: &str) -> Vec<ValueNode> {
    let mut changed = false;
    let mut result = Vec::new();
    for node in param {
        if !changed && node.is_word(origin) {
            result.push(ValueNode::Word(name.to_string()));
            changed = true;
        } else {
            result.push(node.clone());
        }
    }
    result
}

fn disabled(ctx: &Session, prop: &str, prefix: &str) -> bool {
    if prop.contains("flex")
        || ["order", "justify-content", "align-self", "align-content"].contains(&prop)
    {
        if ctx.prefixes.options.flexbox == crate::engine::Flexbox::None {
            return true;
        }
        if ctx.prefixes.options.flexbox == crate::engine::Flexbox::No2009 {
            return prefix.contains("2009");
        }
    }
    false
}

fn div(params: &[Vec<ValueNode>]) -> ValueNode {
    for param in params {
        for node in param {
            if let ValueNode::Div { value, .. } = node
                && value == ","
            {
                return node.clone();
            }
        }
    }
    ValueNode::Div {
        value: String::from(","),
        before: String::new(),
        after: String::from(" "),
    }
}

pub(crate) fn find_prop(param: &[ValueNode]) -> String {
    let Some(first) = param.first() else {
        return String::new();
    };
    let prop = first.value().to_string();
    if prop.starts_with(|c: char| c.is_ascii_digit()) {
        for node in param.iter().skip(1) {
            if matches!(node, ValueNode::Word(_)) {
                return node.value().to_string();
            }
        }
    }
    prop
}

pub(crate) fn parse(text: &str) -> Vec<Vec<ValueNode>> {
    let mut result: Vec<Vec<ValueNode>> = Vec::new();
    let mut param: Vec<ValueNode> = Vec::new();
    for node in value::parse(text) {
        let divider = matches!(&node, ValueNode::Div { value, .. } if value == ",");
        param.push(node);
        if divider {
            result.push(std::mem::take(&mut param));
        }
    }
    result.push(param);
    result
        .into_iter()
        .filter(|param| !param.is_empty())
        .collect()
}

fn stringify(params: &mut [Vec<ValueNode>]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let separator = div(params);
    let mut nodes: Vec<ValueNode> = Vec::new();
    for param in params.iter_mut() {
        if !matches!(param.last(), Some(ValueNode::Div { .. })) {
            param.push(separator.clone());
        }
        nodes.extend(param.iter().cloned());
    }
    if matches!(nodes.first(), Some(ValueNode::Div { .. })) {
        nodes.remove(0);
    }
    if matches!(nodes.last(), Some(ValueNode::Div { .. })) {
        nodes.pop();
    }
    value::stringify(&nodes)
}

fn rule_vendor_prefixes(ctx: &Session, decl: NodeId) -> Option<Vec<String>> {
    let parent = ctx.tree.parent(decl)?;
    if ctx.tree.kind(parent) != NodeKind::Rule {
        return None;
    }
    let selector = ctx.tree.selector(parent);
    if !selector.contains(":-") {
        return None;
    }
    let selectors: Vec<String> = crate::browsers::Browsers::prefixes()
        .iter()
        .filter(|prefix| selector.contains(&format!(":{prefix}")))
        .cloned()
        .collect();
    (!selectors.is_empty()).then_some(selectors)
}
