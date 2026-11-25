use std::sync::OnceLock;

use crate::data::{AGENTS, Agent};

pub(crate) static DEFAULTS: &[&str] = &["> 0.5%", "last 2 versions", "Firefox ESR", "not dead"];
static DEAD: &[&str] = &[
    "Baidu >= 0",
    "ie <= 11",
    "ie_mob <= 11",
    "bb <= 10",
    "op_mob <= 12.1",
    "samsung 4",
];

const ANDROID_EVERGREEN_FIRST: &str = "37";
const OP_MOB_BLINK_FIRST: f64 = 14.0;
const FIREFOX_ESR_VERSION: &str = "140";
const YEAR_MS: f64 = 365.259641 * 24.0 * 60.0 * 60.0 * 1000.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryError(pub String);

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for QueryError {}

fn error(text: impl Into<String>) -> QueryError {
    QueryError(text.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compose {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    query: String,
    kind: &'static str,
    not: bool,
    compose: Compose,
    captures: Vec<String>,
}

pub(crate) struct Browsers {
    pub selected: Vec<String>,
}

impl Browsers {
    pub(crate) fn new(requirements: Option<&[String]>) -> Result<Self, QueryError> {
        let selected = match requirements {
            Some(list) => resolve_queries(list)?,
            None => resolve_queries(&to_strings(DEFAULTS))?,
        };
        Ok(Self { selected })
    }

    pub(crate) fn with_selected(selected: Vec<String>) -> Self {
        Self { selected }
    }

    pub(crate) fn is_selected(&self, browser: &str) -> bool {
        self.selected.iter().any(|selected| selected == browser)
    }

    pub(crate) fn prefix(browser: &str) -> String {
        let (name, version) = browser.split_once(' ').unwrap_or((browser, ""));
        let agent = agent(name).expect("browser is known");
        let prefix = agent
            .prefix_exceptions
            .iter()
            .find(|(at, _)| *at == version)
            .map_or(agent.prefix, |(_, prefix)| *prefix);
        format!("-{prefix}-")
    }

    pub(crate) fn prefixes() -> &'static [String] {
        static PREFIXES: OnceLock<Vec<String>> = OnceLock::new();
        PREFIXES.get_or_init(|| {
            let mut prefixes: Vec<String> = Vec::new();
            for agent in AGENTS {
                let prefix = format!("-{}-", agent.prefix);
                if !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
            prefixes.sort_by_key(|prefix: &String| std::cmp::Reverse(prefix.len()));
            prefixes
        })
    }

    pub(crate) fn with_prefix(value: &str) -> bool {
        Self::prefixes().iter().any(|prefix| value.contains(prefix))
    }
}

pub(crate) fn resolve_queries(queries: &[String]) -> Result<Vec<String>, QueryError> {
    let nodes = parse(queries);
    let mut result: Vec<String> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.not && index == 0 {
            return Err(error(format!(
                "Write any browsers query (for instance, `defaults`) before `{}`",
                if node.not {
                    format!("not {}", node.query)
                } else {
                    node.query.clone()
                }
            )));
        }
        let mut array = select(node)?;
        for item in &mut array {
            if let Some((name, "0")) = item.split_once(' ')
                && let Some(data) = by_name(name)
                && let Some(first) = data.versions.first()
            {
                *item = format!("{name} {first}");
            }
        }
        let contains = |item: &String| array.contains(item);
        match node.compose {
            Compose::And if node.not => result.retain(|item| !contains(item)),
            Compose::And => result.retain(contains),
            Compose::Or if node.not => result.retain(|item| !contains(item)),
            Compose::Or => result.extend(array),
        }
    }
    let mut unique: Vec<String> = Vec::new();
    for item in result {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique.sort_by(|a, b| compare_browsers(a, b));
    Ok(unique)
}

fn compare_browsers(first: &str, second: &str) -> std::cmp::Ordering {
    let (name_a, version_a) = first.split_once(' ').unwrap_or((first, ""));
    let (name_b, version_b) = second.split_once(' ').unwrap_or((second, ""));
    if name_a == name_b {
        let a = version_a.split('-').next().unwrap_or("");
        let b = version_b.split('-').next().unwrap_or("");
        return compare_semver(b, a);
    }
    name_a.cmp(name_b)
}

fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let a: Vec<f64> = a.split('.').map(parse_int).collect();
    let b: Vec<f64> = b.split('.').map(parse_int).collect();
    compare(
        a.first().copied().unwrap_or(f64::NAN),
        b.first().copied().unwrap_or(f64::NAN),
    )
    .then_with(|| {
        compare(
            a.get(1).copied().unwrap_or(0.0),
            b.get(1).copied().unwrap_or(0.0),
        )
    })
    .then_with(|| {
        compare(
            a.get(2).copied().unwrap_or(0.0),
            b.get(2).copied().unwrap_or(0.0),
        )
    })
}

fn compare(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

fn parse_int(text: &str) -> f64 {
    let text = text.trim_start_matches(|c: char| c.is_whitespace());
    let bytes = text.as_bytes();
    let mut at = usize::from(bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+'));
    let start = at;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
    }
    if at == start {
        return f64::NAN;
    }
    text[start..at].parse().unwrap_or(f64::NAN)
}

pub(crate) fn parse_float(text: &str) -> f64 {
    let text = text.trim_start_matches(|c: char| c.is_whitespace());
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

fn agent(name: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|agent| agent.name == name)
}

fn alias(name: &str) -> &str {
    match name {
        "fx" | "ff" => "firefox",
        "ios" => "ios_saf",
        "explorer" => "ie",
        "blackberry" => "bb",
        "explorermobile" => "ie_mob",
        "operamini" => "op_mini",
        "operamobile" => "op_mob",
        "chromeandroid" => "and_chr",
        "firefoxandroid" => "and_ff",
        "ucandroid" => "and_uc",
        "qqandroid" => "and_qq",
        other => other,
    }
}

fn by_name(name: &str) -> Option<&'static Agent> {
    let name = name.to_lowercase();
    agent(alias(&name))
}

fn check_name(name: &str) -> Result<&'static Agent, QueryError> {
    by_name(name).ok_or_else(|| error(format!("Unknown browser {name}")))
}

fn version_alias(data: &Agent, version: &str) -> Option<String> {
    if version == "esr" && data.name == "firefox" {
        return Some(String::from(FIREFOX_ESR_VERSION));
    }
    data.versions
        .iter()
        .filter(|full| full.contains('-'))
        .find_map(|full| {
            full.split('-')
                .any(|part| part == version)
                .then(|| full.to_string())
        })
}
fn known_versions(data: &Agent) -> impl Iterator<Item = &'static str> {
    data.versions
        .iter()
        .copied()
        .filter(|version| !version.is_empty())
}

fn tail(length: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        length.saturating_sub(count)
    }
}

fn resolve_version(data: &Agent, version: &str) -> Option<String> {
    if known_versions(data).any(|known| known == version) {
        return Some(version.to_string());
    }
    version_alias(data, version)
}

fn normalize_version(data: &Agent, version: &str) -> Option<String> {
    if let Some(resolved) = resolve_version(data, version) {
        return Some(resolved);
    }
    let mut known = known_versions(data);
    if let (Some(only), None) = (known.next(), known.next()) {
        return Some(only.to_string());
    }
    None
}

fn get_major(version: &str) -> f64 {
    parse_int(version.split('.').next().unwrap_or(""))
}

fn get_major_versions(released: &[&str], number: usize) -> Vec<String> {
    if released.is_empty() {
        return Vec::new();
    }
    let mut majors: Vec<f64> = Vec::new();
    for version in released {
        let major = get_major(version);
        if !majors.contains(&major) {
            majors.push(major);
        }
    }
    let index = majors.len() as isize - number as isize;
    if index < 0 {
        return released.iter().map(|version| version.to_string()).collect();
    }
    let minimum = majors[index as usize];
    let mut selected: Vec<String> = Vec::new();
    for version in released.iter().rev() {
        if compare(minimum, get_major(version)) == std::cmp::Ordering::Greater {
            break;
        }
        selected.insert(0, version.to_string());
    }
    selected
}

fn filter_jumps(list: Vec<String>, name: &str, versions: usize) -> Vec<String> {
    let jump = match name {
        "android" => {
            let chrome = agent("chrome").expect("chrome is known");
            let at = chrome
                .released
                .iter()
                .position(|version| *version == ANDROID_EVERGREEN_FIRST)
                .map_or(-1isize, |at| at as isize);
            (chrome.released.len() as isize - at) as usize
        }
        "op_mob" => {
            let data = agent("op_mob").expect("op_mob is known");
            let latest = data.released.last().copied().unwrap_or("");
            let jump = get_major(latest) - OP_MOB_BLINK_FIRST + 1.0;
            if jump.is_nan() || jump < 0.0 {
                0
            } else {
                jump as usize
            }
        }
        _ => return list,
    };
    if versions <= jump {
        return list.last().cloned().into_iter().collect();
    }
    let keep = versions - jump + 1;
    let start = list.len().saturating_sub(keep);
    list[start..].to_vec()
}

fn filter_by_year(since_seconds: f64) -> Vec<String> {
    let mut selected = Vec::new();
    for data in AGENTS {
        for (version, date) in data.release {
            if *date >= 0 && (*date as f64) >= since_seconds {
                selected.push(format!("{} {}", data.name, version));
            }
        }
    }
    selected
}

fn since_query(year: &str, month: Option<&str>, day: Option<&str>) -> Vec<String> {
    let year = parse_int(year);
    let month = parse_int(month.unwrap_or("01")) - 1.0;
    let day = parse_int(day.unwrap_or("01"));
    let since = utc_seconds(year, month, day);
    filter_by_year(since)
}

fn utc_seconds(year: f64, month: f64, day: f64) -> f64 {
    let year = if (0.0..=99.0).contains(&year) {
        year + 1900.0
    } else {
        year
    };
    let year = year.floor() as i64;
    let month = (month.floor() as i64).rem_euclid(12) as u32 + 1;
    let day = (day.floor() as i64).clamp(1, 31) as u32;
    days_from_civil(year, month, day) as f64 * 86_400.0
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn usage_global() -> Vec<(String, f64)> {
    static USAGE: OnceLock<Vec<(String, f64)>> = OnceLock::new();
    USAGE
        .get_or_init(|| {
            let mut usage = Vec::new();
            for data in AGENTS {
                for (version, percent) in data.usage {
                    usage.push((format!("{} {}", data.name, version), *percent));
                }
            }
            usage
        })
        .clone()
}

fn generate_filter(sign: &str, version: f64) -> impl Fn(&str) -> bool {
    move |v: &str| {
        let latest = parse_float(v.split('-').nth(1).unwrap_or(v));
        match sign {
            ">" => latest > version,
            ">=" => latest >= version,
            "<" => parse_float(v) < version,
            _ => parse_float(v) <= version,
        }
    }
}

fn popularity(sign: &str, percent: f64) -> Vec<String> {
    usage_global()
        .into_iter()
        .filter(|(_, usage)| match sign {
            ">" => *usage > percent,
            "<" => *usage < percent,
            "<=" => *usage <= percent,
            _ => *usage >= percent,
        })
        .map(|(version, _)| version)
        .collect()
}

fn cover(coverage: f64) -> Vec<String> {
    let mut versions = usage_global();
    versions.sort_by(|a, b| compare(b.1, a.1));
    let mut covered = 0.0;
    let mut result = Vec::new();
    for (version, usage) in versions {
        if usage == 0.0 {
            break;
        }
        covered += usage;
        result.push(version);
        if covered >= coverage {
            break;
        }
    }
    result
}

fn select(node: &Node) -> Result<Vec<String>, QueryError> {
    let c = &node.captures;
    let number = |at: usize| c[at].parse::<usize>().unwrap_or(0);
    match node.kind {
        "last_major_versions" => {
            let mut selected = Vec::new();
            for data in AGENTS {
                let list = get_major_versions(data.released, number(0))
                    .into_iter()
                    .map(|version| format!("{} {}", data.name, version))
                    .collect();
                selected.extend(filter_jumps(list, data.name, number(0)));
            }
            Ok(selected)
        }
        "last_versions" => {
            let mut selected = Vec::new();
            for data in AGENTS {
                let start = tail(data.released.len(), number(0));
                let list: Vec<String> = data.released[start..]
                    .iter()
                    .map(|version| format!("{} {}", data.name, version))
                    .collect();
                selected.extend(filter_jumps(list, data.name, number(0)));
            }
            Ok(selected)
        }
        "last_browser_major_versions" => {
            let data = check_name(&c[1])?;
            let list = get_major_versions(data.released, number(0))
                .into_iter()
                .map(|version| format!("{} {}", data.name, version))
                .collect();
            Ok(filter_jumps(list, data.name, number(0)))
        }
        "last_browser_versions" => {
            let data = check_name(&c[1])?;
            let start = tail(data.released.len(), number(0));
            let list: Vec<String> = data.released[start..]
                .iter()
                .map(|version| format!("{} {}", data.name, version))
                .collect();
            Ok(filter_jumps(list, data.name, number(0)))
        }
        "unreleased_versions" => {
            let mut selected = Vec::new();
            for data in AGENTS {
                selected.extend(
                    known_versions(data)
                        .filter(|version| !data.released.contains(version))
                        .map(|version| format!("{} {}", data.name, version)),
                );
            }
            Ok(selected)
        }
        "unreleased_browser_versions" => {
            let data = check_name(&c[0])?;
            Ok(known_versions(data)
                .filter(|version| !data.released.contains(version))
                .map(|version| format!("{} {}", data.name, version))
                .collect())
        }
        "last_years" => {
            let years = parse_float(&c[0]);
            Ok(filter_by_year(now_seconds() - YEAR_MS * years / 1000.0))
        }
        "since_y" => Ok(since_query(&c[0], None, None)),
        "since_y_m" => Ok(since_query(&c[0], Some(&c[1]), None)),
        "since_y_m_d" => Ok(since_query(&c[0], Some(&c[1]), Some(&c[2]))),
        "popularity" => Ok(popularity(&c[0], parse_float(&c[1]))),
        "cover" => Ok(cover(parse_float(&c[0]))),
        "browser_range" => {
            let data = check_name(&c[0])?;
            let from = parse_float(&normalize_version(data, &c[1]).unwrap_or_else(|| c[1].clone()));
            let to = parse_float(&normalize_version(data, &c[2]).unwrap_or_else(|| c[2].clone()));
            Ok(data
                .released
                .iter()
                .filter(|version| {
                    let parsed = parse_float(version);
                    parsed >= from && parsed <= to
                })
                .map(|version| format!("{} {}", data.name, version))
                .collect())
        }
        "browser_ray" => {
            let data = check_name(&c[0])?;
            let mut version = c[2].clone();
            if let Some(alias) = version_alias(data, &version.to_lowercase()) {
                version = alias;
            }
            if !version.contains(|c: char| c.is_ascii_digit() || c == '.') {
                return Err(error(format!("Unknown version {version} of {}", c[0])));
            }
            let filter = generate_filter(&c[1], parse_float(&version));
            Ok(data
                .released
                .iter()
                .filter(|version| filter(version))
                .map(|version| format!("{} {}", data.name, version))
                .collect())
        }
        "firefox_esr" => Ok(vec![format!("firefox {FIREFOX_ESR_VERSION}")]),
        "opera_mini_all" => Ok(vec![String::from("op_mini all")]),
        "phantomjs_1_9" => Ok(vec![String::from("safari 5")]),
        "phantomjs_2_1" => Ok(vec![String::from("safari 6")]),
        "browser_version" => {
            let data = check_name(&c[0])?;
            let mut version = if c[1].eq_ignore_ascii_case("tp") {
                String::from("TP")
            } else {
                c[1].clone()
            };
            match normalize_version(data, &version) {
                Some(alias) => version = alias,
                None => {
                    let alias = if !version.contains('.') {
                        format!("{version}.0")
                    } else {
                        version.strip_suffix(".0").unwrap_or(&version).to_string()
                    };
                    match normalize_version(data, &alias) {
                        Some(alias) => version = alias,
                        None => {
                            return Err(error(format!("Unknown version {version} of {}", c[0])));
                        }
                    }
                }
            }
            Ok(vec![format!("{} {}", data.name, version)])
        }
        "defaults" => resolve_queries(&to_strings(DEFAULTS)),
        "dead" => resolve_queries(&to_strings(DEAD)),
        "electron_range"
        | "last_electron_major_versions"
        | "last_electron_versions"
        | "unreleased_electron_versions"
        | "electron_ray"
        | "electron_version" => Err(error(
            "Electron queries are not supported by this browserslist engine",
        )),
        "node_range"
        | "last_node_major_versions"
        | "last_node_versions"
        | "node_ray"
        | "node_major_version"
        | "node_minor_version"
        | "node_patch_version"
        | "current_node"
        | "maintained_node" => Err(error(
            "Node.js queries are not supported by this browserslist engine",
        )),
        "baseline" => Err(error(
            "`baseline` queries are not supported by this browserslist engine",
        )),
        "popularity_in_place"
        | "cover_in"
        | "popularity_in_my_stats"
        | "popularity_in_config_stats"
        | "cover_config" => Err(error(
            "Regional and custom usage statistics are not supported by this browserslist engine",
        )),
        "supports" => Err(error(
            "`supports` queries are not supported by this browserslist engine",
        )),
        "browserslist_config" | "extends" => Err(error(
            "Config file queries are not supported by this browserslist engine",
        )),
        "unknown" => {
            if by_name(&node.query).is_some() {
                Err(error(format!(
                    "Specify versions in Browserslist query for browser {}",
                    node.query
                )))
            } else {
                Err(error(format!(
                    "Unknown browser query `{}`. Maybe you are using old Browserslist or made typo in query.",
                    node.query
                )))
            }
        }
        other => Err(error(format!("Unknown browser query `{other}`"))),
    }
}

fn to_strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

fn now_seconds() -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64() * 1000.0);
    now / 1000.0
}

fn parse(queries: &[String]) -> Vec<Node> {
    let mut nodes = Vec::new();
    for block in queries {
        let mut qs: Vec<Node> = Vec::new();
        let mut rest = block.clone();
        while let Some((head, node)) = match_block(&rest) {
            qs.insert(0, node);
            rest = head;
        }
        nodes.extend(qs);
    }
    nodes
}

fn match_block(block: &str) -> Option<(String, Node)> {
    let chars: Vec<char> = block.chars().collect();
    let max = chars.len();
    for n in 1..=max {
        let parsed: String = chars[max - n..].iter().collect();
        if let Some(node) = split_compose(&parsed, true) {
            return Some((chars[..max - n].iter().collect(), node));
        }
        if let Some(node) = split_compose(&parsed, false) {
            return Some((chars[..max - n].iter().collect(), node));
        }
        if n == max {
            return Some((
                String::new(),
                with_compose(match_query(parsed.trim()), Compose::Or),
            ));
        }
    }
    None
}

fn split_compose(parsed: &str, is_and: bool) -> Option<Node> {
    static AND: OnceLock<regex::Regex> = OnceLock::new();
    static OR: OnceLock<regex::Regex> = OnceLock::new();
    let (pattern, compose) = if is_and {
        (
            AND.get_or_init(|| regex::Regex::new(r"^\s+and\s+(.*)$").unwrap()),
            Compose::And,
        )
    } else {
        (
            OR.get_or_init(|| regex::Regex::new(r"^(?:,\s*|\s+or\s+)(.*)$").unwrap()),
            Compose::Or,
        )
    };
    let captures = pattern.captures(parsed)?;
    Some(with_compose(match_query(&captures[1]), compose))
}

fn with_compose(mut node: Node, compose: Compose) -> Node {
    node.compose = compose;
    node
}

const WORD: &str = "[0-9A-Za-z_]";
const NUMBER: &str = r"(?:\d+|\d+\.\d+|\.\d+)";

fn patterns() -> &'static Vec<(&'static str, regex::Regex)> {
    static PATTERNS: OnceLock<Vec<(&'static str, regex::Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let source: &[(&str, &str)] = &[
            ("last_major_versions", r"(?i)^last\s+(\d+)\s+major\s+versions?$"),
            ("last_versions", r"(?i)^last\s+(\d+)\s+versions?$"),
            ("last_electron_major_versions", r"(?i)^last\s+(\d+)\s+electron\s+major\s+versions?$"),
            ("last_node_major_versions", r"(?i)^last\s+(\d+)\s+node\s+major\s+versions?$"),
            ("last_browser_major_versions", r"(?i)^last\s+(\d+)\s+(WORD+)\s+major\s+versions?$"),
            ("last_electron_versions", r"(?i)^last\s+(\d+)\s+electron\s+versions?$"),
            ("last_node_versions", r"(?i)^last\s+(\d+)\s+node\s+versions?$"),
            ("last_browser_versions", r"(?i)^last\s+(\d+)\s+(WORD+)\s+versions?$"),
            ("unreleased_versions", r"(?i)^unreleased\s+versions$"),
            ("unreleased_electron_versions", r"(?i)^unreleased\s+electron\s+versions?$"),
            ("unreleased_browser_versions", r"(?i)^unreleased\s+(WORD+)\s+versions?$"),
            ("last_years", r"(?i)^last\s+((?:\d+\.)?\d+)\s+years?$"),
            ("since_y", r"^since (\d+)$"),
            ("since_y_m", r"^since (\d+)-(\d+)$"),
            ("since_y_m_d", r"^since (\d+)-(\d+)-(\d+)$"),
            ("baseline", r"(?i)^baseline\s+(?:(\d+)|(newly|widely)\s+available(?:\s+on\s+(\d{4}-\d{2}-\d{2}))?)?(\s+with\s+downstream)?(\s+including\s+kaios)?$"),
            ("popularity", "^(>=?|<=?)\\s*(NUMBER)%$"),
            ("popularity_in_my_stats", "^(>=?|<=?)\\s*(NUMBER)%\\s+in\\s+my\\s+stats$"),
            ("popularity_in_config_stats", "^(>=?|<=?)\\s*(NUMBER)%\\s+in\\s+(\\S+)\\s+stats$"),
            ("popularity_in_place", "^(>=?|<=?)\\s*(NUMBER)%\\s+in\\s+((alt-)?\\w\\w)$"),
            ("cover", r"(?i)^cover\s+(NUMBER)%$"),
            ("cover_in", r"(?i)^cover\s+(NUMBER)%\s+in\s+(my\s+stats|(alt-)?\w\w)$"),
            ("cover_config", r"(?i)^cover\s+(NUMBER)%\s+in\s+(\S+)\s+stats$"),
            ("supports", r"^(?:(fully|partially)\s+)?supports\s+([\w-]+)$"),
            ("electron_range", r"(?i)^electron\s+([\d.]+)\s*-\s*([\d.]+)$"),
            ("node_range", r"(?i)^node\s+([\d.]+)\s*-\s*([\d.]+)$"),
            ("browser_range", r"(?i)^(WORD+)\s+([\d.]+)\s*-\s*([\d.]+)$"),
            ("electron_ray", r"(?i)^electron\s*(>=?|<=?)\s*([\d.]+)$"),
            ("node_ray", r"(?i)^node\s*(>=?|<=?)\s*([\d.]+)$"),
            ("browser_ray", r"(?i)^(WORD+)\s*(>=?|<=?)\s*([\d.]+|esr)$"),
            ("firefox_esr", r"(?i)^(firefox|ff|fx)\s+esr$"),
            ("opera_mini_all", r"(?i)(operamini|op_mini)\s+all"),
            ("electron_version", r"(?i)^electron\s+([\d.]+)$"),
            ("node_major_version", r"(?i)^node\s+(\d+)$"),
            ("node_minor_version", r"(?i)^node\s+(\d+\.\d+)$"),
            ("node_patch_version", r"(?i)^node\s+(\d+\.\d+\.\d+)$"),
            ("current_node", r"(?i)^current\s+node$"),
            ("maintained_node", r"(?i)^maintained\s+node\s+versions$"),
            ("phantomjs_1_9", r"(?i)^phantomjs\s+1\.9$"),
            ("phantomjs_2_1", r"(?i)^phantomjs\s+2\.1$"),
            ("browser_version", r"(?i)^(WORD+)\s+(tp|[\d.]+)$"),
            ("browserslist_config", r"(?i)^browserslist config$"),
            ("extends", r"(?i)^extends (.+)$"),
            ("defaults", r"(?i)^defaults$"),
            ("dead", r"(?i)^dead$"),
            ("unknown", r"(?i)^(WORD+)$"),
        ];
        source
            .iter()
            .map(|(kind, source)| {
                let source = source.replace("NUMBER", NUMBER).replace("WORD", WORD);
                (*kind, regex::Regex::new(&source).expect("query pattern is valid"))
            })
            .collect()
    })
}

fn match_query(query: &str) -> Node {
    let mut not = false;
    let mut query = query.to_string();
    if let Some(rest) = query.strip_prefix("not ") {
        not = true;
        query = rest.to_string();
    }
    for (kind, pattern) in patterns() {
        if let Some(captures) = pattern.captures(&query) {
            let captures = captures
                .iter()
                .skip(1)
                .map(|group| group.map_or_else(String::new, |text| text.as_str().to_string()))
                .collect();
            return Node {
                query,
                kind,
                not,
                compose: Compose::Or,
                captures,
            };
        }
    }
    Node {
        query,
        kind: "unknown",
        not,
        compose: Compose::Or,
        captures: Vec::new(),
    }
}
