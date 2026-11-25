use std::sync::OnceLock;

use postcss_guru::{NodeId, NodeKind};

use crate::declaration::*;
use crate::engine::Session;
use crate::utils::{space, split_whitespace};

struct AlignContent;

impl DeclHack for AlignContent {
    fn normalize(&self, _prop: &str) -> String {
        String::from("align-content")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        if spec == Spec::V2012 {
            return format!("{prefix}flex-line-pack");
        }
        format!("{prefix}{prop}")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        if spec == Spec::V2012 {
            let value = old_value(
                &[
                    ("flex-end", "end"),
                    ("flex-start", "start"),
                    ("space-around", "distribute"),
                    ("space-between", "justify"),
                ],
                ctx.tree.value(node),
            );
            ctx.tree.set_value(node, value);
            let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
            ctx.tree.set_prop(node, prefixed);
            return Some(node);
        }
        if spec == Spec::Final {
            let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
            ctx.tree.set_prop(node, prefixed);
            return Some(node);
        }
        None
    }
}

struct AlignItems;

impl DeclHack for AlignItems {
    fn normalize(&self, _prop: &str) -> String {
        String::from("align-items")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        match spec {
            Spec::V2009 => format!("{prefix}box-align"),
            Spec::V2012 => format!("{prefix}flex-align"),
            _ => format!("{prefix}{prop}"),
        }
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        if matches!(spec, Spec::V2009 | Spec::V2012) {
            let value = old_value(
                &[("flex-end", "end"), ("flex-start", "start")],
                ctx.tree.value(node),
            );
            ctx.tree.set_value(node, value);
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct AlignSelf;

impl DeclHack for AlignSelf {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        ctx.tree.parent(node).is_some()
            && !ctx.parent_has(node, |tree, child| tree.prop(child).starts_with("grid-"))
    }

    fn normalize(&self, _prop: &str) -> String {
        String::from("align-self")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        if spec == Spec::V2012 {
            return format!("{prefix}flex-item-align");
        }
        format!("{prefix}{prop}")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        match spec {
            Spec::V2012 => {
                let value = old_value(
                    &[("flex-end", "end"), ("flex-start", "start")],
                    ctx.tree.value(node),
                );
                ctx.tree.set_value(node, value);
                let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
                ctx.tree.set_prop(node, prefixed);
                Some(node)
            }
            Spec::Final => {
                let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
                ctx.tree.set_prop(node, prefixed);
                Some(node)
            }
            _ => None,
        }
    }
}

struct Animation;

impl DeclHack for Animation {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        !split_whitespace(ctx.tree.value(node)).iter().any(|word| {
            let lower = word.to_lowercase();
            lower == "reverse" || lower == "alternate-reverse"
        })
    }
}

struct Appearance;

impl DeclHack for Appearance {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        map_prefixes(prefixes, "-ms-", "-webkit-")
    }
}

struct BackdropFilter;

impl DeclHack for BackdropFilter {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        map_prefixes(prefixes, "-ms-", "-webkit-")
    }
}

struct BackgroundClip;

impl DeclHack for BackgroundClip {
    fn prefixes(&self, prefixes: Vec<String>) -> Vec<String> {
        map_prefixes(prefixes, "-ms-", "-webkit-")
    }

    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        ctx.tree.value(node).to_lowercase() == "text"
    }
}

struct BackgroundSize;

impl DeclHack for BackgroundSize {
    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let value = ctx.tree.value(node).to_lowercase();
        if prefix == "-webkit-" && !value.contains(' ') && value != "contain" && value != "cover" {
            let original = ctx.tree.value(node).to_string();
            let doubled = format!("{original} {original}");
            ctx.tree.set_value(node, doubled);
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct BlockLogical;

impl DeclHack for BlockLogical {
    fn normalize(&self, prop: &str) -> String {
        if prop.contains("-before") {
            replace_first(prop, "-before", "-block-start")
        } else {
            replace_first(prop, "-after", "-block-end")
        }
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        if prop.contains("-start") {
            format!("{prefix}{}", replace_first(prop, "-block-start", "-before"))
        } else {
            format!("{prefix}{}", replace_first(prop, "-block-end", "-after"))
        }
    }
}

struct BorderImage;

impl DeclHack for BorderImage {
    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        static FILL: OnceLock<regex::Regex> = OnceLock::new();
        let regex = FILL.get_or_init(|| regex::Regex::new(r"\s+fill(\s)").unwrap());
        let value = regex.replace(ctx.tree.value(node), "$1").into_owned();
        ctx.tree.set_value(node, value);
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct BorderRadius;

impl DeclHack for BorderRadius {
    fn normalize(&self, prop: &str) -> String {
        radius_names()
            .iter()
            .find(|(_, mozilla)| *mozilla == prop)
            .map_or_else(|| prop.to_string(), |(normal, _)| normal.to_string())
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        if prefix == "-moz-" {
            let mozilla = radius_names()
                .iter()
                .find(|(normal, _)| *normal == prop)
                .map_or(prop, |(_, mozilla)| mozilla.as_str());
            return format!("{prefix}{mozilla}");
        }
        format!("{prefix}{prop}")
    }
}

fn radius_names() -> &'static Vec<(String, String)> {
    static NAMES: OnceLock<Vec<(String, String)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = Vec::new();
        for side in ["top", "bottom"] {
            for corner in ["left", "right"] {
                names.push((
                    format!("border-{side}-{corner}-radius"),
                    format!("border-radius-{side}{corner}"),
                ));
            }
        }
        names
    })
}

struct BreakProps;

impl DeclHack for BreakProps {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        let value = ctx.tree.value(node).to_string();
        if ctx.tree.prop(node) == "break-inside"
            && (value.to_lowercase().contains("region") || value.to_lowercase().contains("page"))
        {
            return None;
        }
        base_insert(self, decl, ctx, node, prefix, prefixes)
    }

    fn normalize(&self, prop: &str) -> String {
        if prop.contains("inside") {
            String::from("break-inside")
        } else if prop.contains("before") {
            String::from("break-before")
        } else {
            String::from("break-after")
        }
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        format!("{prefix}column-{prop}")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let value = ctx.tree.value(node).to_string();
        if ctx.tree.prop(node) == "break-inside" && value == "avoid-column" || value == "avoid-page"
        {
            ctx.tree.set_value(node, "avoid");
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct FlexBasis;

impl DeclHack for FlexBasis {
    fn normalize(&self, _prop: &str) -> String {
        String::from("flex-basis")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        if spec == Spec::V2012 {
            return format!("{prefix}flex-preferred-size");
        }
        format!("{prefix}{prop}")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        if matches!(spec, Spec::V2012 | Spec::Final) {
            let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
            ctx.tree.set_prop(node, prefixed);
            return Some(node);
        }
        None
    }
}

fn box_pair(
    decl: &Decl,
    ctx: &mut Session,
    node: NodeId,
    prefix: &str,
    prefixes: &[String],
    orient: String,
    dir: String,
) -> Option<NodeId> {
    let mut inserted = None;
    for (suffix, value) in [("box-orient", orient), ("box-direction", dir)] {
        let cloned = ctx.clone(node);
        ctx.tree.set_prop(cloned, format!("{prefix}{suffix}"));
        ctx.tree.set_value(cloned, value);
        if need_cascade(decl, ctx, node) {
            let before = calc_before(decl, ctx, node, prefixes, prefix);
            ctx.tree.raws_mut(cloned).before = Some(before);
        }
        inserted = Some(ctx.tree.insert_before(node, cloned));
    }
    inserted
}

struct FlexDirection;

impl DeclHack for FlexDirection {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        let (spec, prefix) = flex_spec(prefix);
        if spec != Spec::V2009 {
            return base_insert(self, decl, ctx, node, &prefix, prefixes);
        }
        let already = ctx.parent_has(node, |tree, child| {
            tree.prop(child) == format!("{prefix}box-orient")
                || tree.prop(child) == format!("{prefix}box-direction")
        });
        if already {
            return None;
        }
        let value = ctx.tree.value(node).to_string();
        let (orient, dir) = if ["inherit", "initial", "unset"].contains(&value.as_str()) {
            (value.clone(), value)
        } else {
            (
                if value.contains("row") {
                    "horizontal"
                } else {
                    "vertical"
                }
                .to_string(),
                if value.contains("reverse") {
                    "reverse"
                } else {
                    "normal"
                }
                .to_string(),
            )
        };
        box_pair(decl, ctx, node, &prefix, prefixes, orient, dir)
    }

    fn normalize(&self, _prop: &str) -> String {
        String::from("flex-direction")
    }

    fn old(&self, prop: &str, prefix: &str) -> Vec<String> {
        let (spec, prefix) = flex_spec(prefix);
        if spec == Spec::V2009 {
            vec![
                format!("{prefix}box-orient"),
                format!("{prefix}box-direction"),
            ]
        } else {
            vec![self.prefixed(prop, &prefix)]
        }
    }
}

struct FlexFlow;

impl DeclHack for FlexFlow {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        let (spec, prefix) = flex_spec(prefix);
        if spec != Spec::V2009 {
            return base_insert(self, decl, ctx, node, &prefix, prefixes);
        }
        let values: Vec<String> = split_whitespace(ctx.tree.value(node))
            .into_iter()
            .filter(|word| word != "wrap" && word != "nowrap")
            .collect();
        if values.is_empty() {
            return None;
        }
        let already = ctx.parent_has(node, |tree, child| {
            tree.prop(child) == format!("{prefix}box-orient")
                || tree.prop(child) == format!("{prefix}box-direction")
        });
        if already {
            return None;
        }
        let value = values[0].clone();
        let orient = if value.contains("row") {
            "horizontal"
        } else {
            "vertical"
        }
        .to_string();
        let dir = if value.contains("reverse") {
            "reverse"
        } else {
            "normal"
        }
        .to_string();
        box_pair(decl, ctx, node, &prefix, prefixes, orient, dir)
    }
}

struct FlexGrow;

impl DeclHack for FlexGrow {
    fn normalize(&self, _prop: &str) -> String {
        String::from("flex")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        match spec {
            Spec::V2009 => format!("{prefix}box-flex"),
            Spec::V2012 => format!("{prefix}flex-positive"),
            _ => format!("{prefix}{prop}"),
        }
    }
}

struct FlexShrink;

impl DeclHack for FlexShrink {
    fn normalize(&self, _prop: &str) -> String {
        String::from("flex-shrink")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        if spec == Spec::V2012 {
            return format!("{prefix}flex-negative");
        }
        format!("{prefix}{prop}")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        if matches!(spec, Spec::V2012 | Spec::Final) {
            let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
            ctx.tree.set_prop(node, prefixed);
            return Some(node);
        }
        None
    }
}

struct FlexWrap;

impl DeclHack for FlexWrap {
    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        if spec != Spec::V2009 {
            let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
            ctx.tree.set_prop(node, prefixed);
            return Some(node);
        }
        None
    }
}

struct Flex;

impl DeclHack for Flex {
    fn normalize(&self, _prop: &str) -> String {
        String::from("flex")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        if spec == Spec::V2009 {
            return format!("{prefix}box-flex");
        }
        format!("{prefix}{prop}")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        let value = ctx.tree.value(node).to_string();
        if spec == Spec::V2009 {
            let first = space(&value).first().cloned().unwrap_or_default();
            let first = old_value(&[("auto", "1"), ("none", "0")], &first);
            ctx.tree.set_value(node, first);
        } else if spec == Spec::V2012 {
            let components = space(&value);
            if components.len() == 3 && components[2] == "0" {
                let joined = format!("{} {} 0px", components[0], components[1]);
                ctx.tree.set_value(node, joined);
            }
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct Filter;

impl DeclHack for Filter {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        let value = ctx.tree.value(node);
        !value.to_lowercase().contains("alpha(")
            && !value.contains("DXImageTransform.Microsoft")
            && !value.contains("data:image/svg+xml")
    }
}

struct GridColumnAlign;

impl DeclHack for GridColumnAlign {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        let value = ctx.tree.value(node);
        !value.contains("flex-") && value != "baseline"
    }

    fn normalize(&self, _prop: &str) -> String {
        String::from("justify-self")
    }

    fn prefixed(&self, _prop: &str, prefix: &str) -> String {
        format!("{prefix}grid-column-align")
    }
}

struct GridRowAlign;

impl DeclHack for GridRowAlign {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        let value = ctx.tree.value(node);
        !value.contains("flex-") && value != "baseline"
    }

    fn normalize(&self, _prop: &str) -> String {
        String::from("align-self")
    }

    fn prefixed(&self, _prop: &str, prefix: &str) -> String {
        format!("{prefix}grid-row-align")
    }
}

struct GridRowsColumns;

impl DeclHack for GridRowsColumns {
    fn normalize(&self, prop: &str) -> String {
        static TEMPLATE: OnceLock<regex::Regex> = OnceLock::new();
        TEMPLATE
            .get_or_init(|| regex::Regex::new(r"^grid-(rows|columns)").unwrap())
            .replace(prop, "grid-template-$1")
            .into_owned()
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        if prefix == "-ms-" {
            return format!("{prefix}{}", replace_first(prop, "template-", ""));
        }
        format!("{prefix}{prop}")
    }
}

struct GridStart;

impl DeclHack for GridStart {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        let value = ctx.tree.value(node);
        !value.contains('/') && !value.contains("span")
    }

    fn normalize(&self, prop: &str) -> String {
        replace_first(prop, "-start", "")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let result = format!("{prefix}{prop}");
        if prefix == "-ms-" {
            return replace_first(&result, "-start", "");
        }
        result
    }
}

struct ImageRendering;

impl DeclHack for ImageRendering {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        ctx.tree.value(node) == "pixelated"
    }

    fn normalize(&self, _prop: &str) -> String {
        String::from("image-rendering")
    }

    fn prefixed(&self, _prop: &str, prefix: &str) -> String {
        if prefix == "-ms-" {
            return String::from("-ms-interpolation-mode");
        }
        format!("{prefix}image-rendering")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        if prefix != "-ms-" {
            let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
            ctx.tree.set_prop(node, prefixed);
            return Some(node);
        }
        ctx.tree.set_prop(node, "-ms-interpolation-mode");
        ctx.tree.set_value(node, "nearest-neighbor");
        Some(node)
    }
}

struct InlineLogical;

impl DeclHack for InlineLogical {
    fn normalize(&self, prop: &str) -> String {
        static LOGICAL: OnceLock<regex::Regex> = OnceLock::new();
        LOGICAL
            .get_or_init(|| regex::Regex::new(r"(margin|padding|border)-(start|end)").unwrap())
            .replace(prop, "$1-inline-$2")
            .into_owned()
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        format!("{prefix}{}", replace_first(prop, "-inline", ""))
    }
}

struct JustifyContent;

impl DeclHack for JustifyContent {
    fn normalize(&self, _prop: &str) -> String {
        String::from("justify-content")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        match spec {
            Spec::V2009 => format!("{prefix}box-pack"),
            Spec::V2012 => format!("{prefix}flex-pack"),
            _ => format!("{prefix}{prop}"),
        }
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        match spec {
            Spec::V2009 | Spec::V2012 => {
                let value = old_value(
                    &[
                        ("flex-end", "end"),
                        ("flex-start", "start"),
                        ("space-around", "distribute"),
                        ("space-between", "justify"),
                    ],
                    ctx.tree.value(node),
                );
                ctx.tree.set_value(node, value.clone());
                if spec != Spec::V2009 || value != "distribute" {
                    let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
                    ctx.tree.set_prop(node, prefixed);
                    return Some(node);
                }
                None
            }
            Spec::Final => {
                let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
                ctx.tree.set_prop(node, prefixed);
                Some(node)
            }
            _ => None,
        }
    }
}

struct MaskBorder;

impl DeclHack for MaskBorder {
    fn normalize(&self, prop: &str) -> String {
        prop.replace("box-image", "border")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let result = format!("{prefix}{prop}");
        if prefix == "-webkit-" {
            return replace_first(&result, "border", "box-image");
        }
        result
    }
}

struct MaskComposite;

static COMPOSITE_VALUES: &[(&str, &str)] = &[
    ("add", "source-over"),
    ("exclude", "xor"),
    ("intersect", "source-in"),
    ("subtract", "source-out"),
];

fn composite_spans(value: &str) -> Vec<(usize, usize)> {
    let letters: Vec<(usize, char)> = value.char_indices().collect();
    let byte = |at: usize| letters.get(at).map_or(value.len(), |(byte, _)| *byte);
    let space = |at: usize| {
        letters
            .get(at)
            .is_some_and(|(_, c)| crate::utils::is_js_space(*c))
    };
    let mut spans = Vec::new();
    let mut at = 0;
    while at < letters.len() {
        if !space(at) {
            at += 1;
            continue;
        }
        let start = at;
        while space(at) {
            at += 1;
        }
        let rest: String = letters[at..].iter().map(|(_, c)| c).collect();
        let lower = rest.to_lowercase();
        let Some(keyword) = ["add", "exclude", "intersect", "subtract"]
            .into_iter()
            .find(|keyword| lower.starts_with(keyword))
        else {
            continue;
        };
        let end = at + keyword.chars().count();
        let boundary = letters
            .get(end)
            .is_none_or(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_');
        if !boundary || letters.get(end).map(|(_, c)| *c) == Some(')') {
            continue;
        }
        let mut after = end;
        while space(after) {
            after += 1;
        }
        if letters.get(after).map(|(_, c)| *c) == Some(',') {
            spans.push((byte(start), byte(after)));
            at = after;
        }
    }
    spans
}

impl DeclHack for MaskComposite {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        let value = ctx.tree.value(node).to_string();
        let is_composite = ctx.tree.prop(node) == "mask-composite";
        let spans = composite_spans(&value);
        let mut composite_values: Vec<String> = if is_composite {
            value.split(',').map(crate::utils::js_trim).collect()
        } else {
            spans
                .iter()
                .map(|(start, end)| crate::utils::js_trim(&value[*start..*end]))
                .collect()
        };
        composite_values.retain(|value| !value.is_empty());
        let has_composite = !composite_values.is_empty();
        let composite = has_composite.then(|| {
            let cloned = ctx.clone(node);
            let mapped: Vec<String> = composite_values
                .iter()
                .map(|value| old_value(COMPOSITE_VALUES, value))
                .collect();
            let mut joined = mapped.join(", ");
            if composite_values.iter().any(|value| value == "intersect") {
                joined.push_str(", xor");
            }
            ctx.tree.set_value(cloned, joined);
            ctx.tree.set_prop(cloned, format!("{prefix}mask-composite"));
            cloned
        });
        let cascade = |ctx: &mut Session, target: NodeId| {
            if need_cascade(decl, ctx, node) {
                let before = calc_before(decl, ctx, node, prefixes, prefix);
                ctx.tree.raws_mut(target).before = Some(before);
            }
        };
        if is_composite {
            let composite = composite?;
            cascade(ctx, composite);
            return Some(ctx.tree.insert_before(node, composite));
        }
        let cloned = ctx.clone(node);
        let prop = format!("{prefix}{}", ctx.tree.prop(cloned));
        ctx.tree.set_prop(cloned, prop);
        if has_composite {
            let mut cleaned = String::new();
            let mut at = 0;
            for (start, end) in &spans {
                cleaned.push_str(&value[at..*start]);
                at = *end;
            }
            cleaned.push_str(&value[at..]);
            ctx.tree.set_value(cloned, cleaned);
        }
        cascade(ctx, cloned);
        ctx.tree.insert_before(node, cloned);
        let Some(composite) = composite else {
            return Some(node);
        };
        cascade(ctx, composite);
        Some(ctx.tree.insert_before(node, composite))
    }
}

struct Order;

impl DeclHack for Order {
    fn normalize(&self, _prop: &str) -> String {
        String::from("order")
    }

    fn prefixed(&self, prop: &str, prefix: &str) -> String {
        let (spec, prefix) = flex_spec(prefix);
        match spec {
            Spec::V2009 => format!("{prefix}box-ordinal-group"),
            Spec::V2012 => format!("{prefix}flex-order"),
            _ => format!("{prefix}{prop}"),
        }
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let (spec, _) = flex_spec(prefix);
        let value = ctx.tree.value(node).to_string();
        if spec == Spec::V2009 && value.chars().any(|c| c.is_ascii_digit()) {
            let number = crate::utils::parse_int(&value) + 1.0;
            ctx.tree.set_value(node, crate::utils::number(number));
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct OverscrollBehavior;

impl DeclHack for OverscrollBehavior {
    fn normalize(&self, _prop: &str) -> String {
        String::from("overscroll-behavior")
    }

    fn prefixed(&self, _prop: &str, prefix: &str) -> String {
        format!("{prefix}scroll-chaining")
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let value = match ctx.tree.value(node) {
            "auto" => String::from("chained"),
            "none" | "contain" => String::from("none"),
            other => other.to_string(),
        };
        ctx.tree.set_value(node, value);
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct PrintColorAdjust;

impl DeclHack for PrintColorAdjust {
    fn normalize(&self, _prop: &str) -> String {
        String::from("print-color-adjust")
    }

    fn prefixed(&self, _prop: &str, prefix: &str) -> String {
        if prefix == "-moz-" {
            return String::from("color-adjust");
        }
        format!("{prefix}print-color-adjust")
    }
}

struct TextDecoration;

impl DeclHack for TextDecoration {
    fn check(&self, ctx: &Session, node: NodeId) -> bool {
        const BASIC: &[&str] = &[
            "none",
            "underline",
            "overline",
            "line-through",
            "blink",
            "inherit",
            "initial",
            "unset",
        ];
        split_whitespace(ctx.tree.value(node))
            .iter()
            .any(|word| !BASIC.contains(&word.as_str()))
    }
}

struct TextDecorationSkipInk;

impl DeclHack for TextDecorationSkipInk {
    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        if ctx.tree.prop(node) == "text-decoration-skip-ink" && ctx.tree.value(node) == "auto" {
            ctx.tree
                .set_prop(node, format!("{prefix}text-decoration-skip"));
            ctx.tree.set_value(node, "ink");
            return Some(node);
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct TextEmphasisPosition;

impl DeclHack for TextEmphasisPosition {
    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        if prefix == "-webkit-" {
            static SIDE: OnceLock<regex::Regex> = OnceLock::new();
            let regex = SIDE.get_or_init(|| regex::Regex::new(r"(?i)\s*(right|left)\s*").unwrap());
            let value = regex.replace(ctx.tree.value(node), "").into_owned();
            ctx.tree.set_value(node, value);
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct TransformDecl;

impl TransformDecl {
    const FUNCTIONS: &'static [&'static str] = &[
        "matrix3d",
        "translate3d",
        "translateZ",
        "scale3d",
        "scaleZ",
        "rotate3d",
        "rotateX",
        "rotateY",
        "perspective",
    ];

    fn contain3d(&self, ctx: &Session, node: NodeId) -> bool {
        if ctx.tree.prop(node) == "transform-origin" {
            return false;
        }
        let value = ctx.tree.value(node);
        Self::FUNCTIONS
            .iter()
            .any(|func| value.contains(&format!("{func}(")))
    }

    fn keyframe_parents(&self, ctx: &Session, node: NodeId) -> bool {
        let mut parent = ctx.tree.parent(node);
        while let Some(current) = parent {
            if ctx.tree.kind(current) == NodeKind::AtRule && ctx.tree.name(current) == "keyframes" {
                return true;
            }
            parent = ctx.tree.parent(current);
        }
        false
    }
}

impl DeclHack for TransformDecl {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        let allowed = match prefix {
            "-ms-" => !self.contain3d(ctx, node) && !self.keyframe_parents(ctx, node),
            "-o-" => !self.contain3d(ctx, node),
            _ => true,
        };
        if allowed {
            return base_insert(self, decl, ctx, node, prefix, prefixes);
        }
        None
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        if prefix == "-ms-" {
            static ROTATE: OnceLock<regex::Regex> = OnceLock::new();
            let regex = ROTATE.get_or_init(|| regex::Regex::new(r"(?i)rotatez").unwrap());
            let value = regex.replace(ctx.tree.value(node), "rotate").into_owned();
            ctx.tree.set_value(node, value);
        }
        Some(node)
    }
}

struct UserSelect;

impl DeclHack for UserSelect {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        let value = ctx.tree.value(node);
        let skipped = value == "all" && prefix == "-ms-"
            || value == "contain" && matches!(prefix, "-moz-" | "-webkit-");
        if skipped {
            return None;
        }
        base_insert(self, decl, ctx, node, prefix, prefixes)
    }

    fn set(&self, ctx: &mut Session, node: NodeId, prefix: &str) -> Option<NodeId> {
        if prefix == "-ms-" && ctx.tree.value(node) == "contain" {
            ctx.tree.set_value(node, "element");
        }
        let prefixed = self.prefixed(ctx.tree.prop(node), prefix);
        ctx.tree.set_prop(node, prefixed);
        Some(node)
    }
}

struct WritingMode;

impl DeclHack for WritingMode {
    fn insert(
        &self,
        decl: &Decl,
        ctx: &mut Session,
        node: NodeId,
        prefix: &str,
        prefixes: &[String],
    ) -> Option<NodeId> {
        if prefix != "-ms-" {
            return base_insert(self, decl, ctx, node, prefix, prefixes);
        }
        let cloned = ctx.clone(node);
        self.set(ctx, cloned, prefix);
        if need_cascade(decl, ctx, node) {
            let before = calc_before(decl, ctx, node, prefixes, prefix);
            ctx.tree.raws_mut(cloned).before = Some(before);
        }
        let mut direction = "ltr";
        let parent = ctx.tree.parent(node);
        if let Some(parent) = parent {
            for child in ctx.tree.children(parent).to_vec() {
                if ctx.tree.prop(child) == "direction"
                    && matches!(ctx.tree.value(child), "rtl" | "ltr")
                {
                    direction = if ctx.tree.value(child) == "rtl" {
                        "rtl"
                    } else {
                        "ltr"
                    };
                }
            }
        }
        let ltr = [
            ("horizontal-tb", "lr-tb"),
            ("vertical-lr", "tb-lr"),
            ("vertical-rl", "tb-rl"),
        ];
        let rtl = [
            ("horizontal-tb", "rl-tb"),
            ("vertical-lr", "bt-lr"),
            ("vertical-rl", "bt-rl"),
        ];
        let table = if direction == "rtl" { rtl } else { ltr };
        let mapped = old_value(&table, ctx.tree.value(node));
        ctx.tree.set_value(cloned, mapped);
        Some(ctx.tree.insert_before(node, cloned))
    }
}

fn old_value(values: &[(&str, &str)], value: &str) -> String {
    values
        .iter()
        .find(|(from, _)| *from == value)
        .map_or_else(|| value.to_string(), |(_, to)| to.to_string())
}

pub(crate) static DECL_HACKS: &[(&[&str], &dyn DeclHack)] = &[
    (&["flex", "box-flex"], &Flex),
    (&["order", "flex-order", "box-ordinal-group"], &Order),
    (&["filter"], &Filter),
    (&["grid-row-end", "grid-column-end"], &Base),
    (&["animation", "animation-direction"], &Animation),
    (&["flex-flow", "box-direction", "box-orient"], &FlexFlow),
    (&["flex-grow", "flex-positive"], &FlexGrow),
    (&["flex-wrap"], &FlexWrap),
    (&["grid-area"], &Base),
    (&["place-self"], &Base),
    (&["grid-row-start", "grid-column-start"], &GridStart),
    (&["align-self", "flex-item-align"], &AlignSelf),
    (&["appearance"], &Appearance),
    (&["flex-basis", "flex-preferred-size"], &FlexBasis),
    (
        &[
            "mask-border",
            "mask-border-source",
            "mask-border-slice",
            "mask-border-width",
            "mask-border-outset",
            "mask-border-repeat",
            "mask-box-image",
            "mask-box-image-source",
            "mask-box-image-slice",
            "mask-box-image-width",
            "mask-box-image-outset",
            "mask-box-image-repeat",
        ],
        &MaskBorder,
    ),
    (&["mask", "mask-composite"], &MaskComposite),
    (&["align-items", "flex-align", "box-align"], &AlignItems),
    (&["user-select"], &UserSelect),
    (&["flex-shrink", "flex-negative"], &FlexShrink),
    (
        &[
            "break-inside",
            "page-break-inside",
            "column-break-inside",
            "break-before",
            "page-break-before",
            "column-break-before",
            "break-after",
            "page-break-after",
            "column-break-after",
        ],
        &BreakProps,
    ),
    (&["writing-mode"], &WritingMode),
    (&["border-image"], &BorderImage),
    (&["align-content", "flex-line-pack"], &AlignContent),
    (
        &[
            "border-radius",
            "border-top-left-radius",
            "border-top-right-radius",
            "border-bottom-right-radius",
            "border-bottom-left-radius",
            "border-radius-topleft",
            "border-radius-topright",
            "border-radius-bottomright",
            "border-radius-bottomleft",
        ],
        &BorderRadius,
    ),
    (
        &[
            "border-block-start",
            "border-block-end",
            "margin-block-start",
            "margin-block-end",
            "padding-block-start",
            "padding-block-end",
            "border-before",
            "border-after",
            "margin-before",
            "margin-after",
            "padding-before",
            "padding-after",
        ],
        &BlockLogical,
    ),
    (&["grid-template"], &Base),
    (
        &[
            "border-inline-start",
            "border-inline-end",
            "margin-inline-start",
            "margin-inline-end",
            "padding-inline-start",
            "padding-inline-end",
            "border-start",
            "border-end",
            "margin-start",
            "margin-end",
            "padding-start",
            "padding-end",
        ],
        &InlineLogical,
    ),
    (&["grid-row-align"], &GridRowAlign),
    (&["transform", "transform-origin"], &TransformDecl),
    (
        &["flex-direction", "box-direction", "box-orient"],
        &FlexDirection,
    ),
    (&["image-rendering", "interpolation-mode"], &ImageRendering),
    (&["backdrop-filter"], &BackdropFilter),
    (&["background-clip"], &BackgroundClip),
    (&["text-decoration"], &TextDecoration),
    (
        &["justify-content", "flex-pack", "box-pack"],
        &JustifyContent,
    ),
    (&["background-size"], &BackgroundSize),
    (&["grid-row", "grid-column"], &Base),
    (
        &[
            "grid-template-rows",
            "grid-template-columns",
            "grid-rows",
            "grid-columns",
        ],
        &GridRowsColumns,
    ),
    (&["grid-column-align"], &GridColumnAlign),
    (
        &["overscroll-behavior", "scroll-chaining"],
        &OverscrollBehavior,
    ),
    (&["grid-template-areas"], &Base),
    (&["print-color-adjust", "color-adjust"], &PrintColorAdjust),
    (&["text-emphasis-position"], &TextEmphasisPosition),
    (
        &["text-decoration-skip-ink", "text-decoration-skip"],
        &TextDecorationSkipInk,
    ),
];
