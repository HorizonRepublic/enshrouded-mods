//! `ensh-mods.toml`, next to the game executable.
//!
//! A small subset of TOML: `[section]` headers, `key = value` lines with an
//! integer, `true` or `false`, or an array of integers, and `#` comments. A file that cannot be
//! read keeps every default and says why; a bad line keeps that key's
//! default.

use std::collections::BTreeMap;

pub const FILE_NAME: &str = "ensh-mods.toml";

/// Vanilla `altarsPerFlameLevel` for Flame levels 0 to 11; later levels
/// repeat the last entry.
const VANILLA_ALTARS: [u8; 12] = [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16];
/// The engine groups only the first 16 zones of the world into bases.
pub const ALTAR_CAP: u8 = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Altars allowed at each Flame level, from level 0; levels past the end
    /// use the last entry.
    pub altars_per_flame_level: Vec<u8>,
    /// A copy of each survivor per altar. Off by default while it is tested.
    pub survivors_per_altar: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            altars_per_flame_level: VANILLA_ALTARS
                .iter()
                .map(|&n| (n * 2).min(ALTAR_CAP))
                .collect(),
            survivors_per_altar: false,
        }
    }
}

impl Config {
    /// Altars allowed at `flame_level`.
    pub fn altars(&self, flame_level: usize) -> u8 {
        let table = &self.altars_per_flame_level;
        table
            .get(flame_level)
            .or(table.last())
            .copied()
            .unwrap_or(ALTAR_CAP)
    }

    /// Parses `text`; the second value lists what was ignored and why.
    pub fn parse(text: &str) -> (Config, Vec<String>) {
        let mut config = Config::default();
        let mut problems = Vec::new();
        let values = match parse_toml(text) {
            Ok(values) => values,
            Err(e) => return (config, vec![e]),
        };
        for ((section, key), value) in values {
            match (section.as_str(), key.as_str(), value) {
                ("flame_altars", "altars_per_flame_level", Value::Array(levels)) => {
                    if levels.is_empty() {
                        problems.push("altars_per_flame_level is empty; using the default".into());
                        continue;
                    }
                    if levels.iter().any(|&n| n > ALTAR_CAP as i64) {
                        problems.push(format!(
                            "altars_per_flame_level above {ALTAR_CAP} lowered to {ALTAR_CAP}: the game groups only {ALTAR_CAP} altar zones into bases"
                        ));
                    }
                    let table: Vec<u8> = levels
                        .iter()
                        .map(|&n| n.clamp(1, ALTAR_CAP as i64) as u8)
                        .collect();
                    config.altars_per_flame_level = table;
                }
                ("survivors", "per_altar", Value::Bool(on)) => {
                    config.survivors_per_altar = on;
                }
                (section, key, _) => {
                    problems.push(format!("unknown or malformed [{section}] {key}; ignored"))
                }
            }
        }
        (config, problems)
    }
}

#[derive(Debug, PartialEq)]
enum Value {
    Integer(i64),
    Bool(bool),
    Array(Vec<i64>),
}

/// `(section, key) -> value`, or the first line that does not parse.
fn parse_toml(text: &str) -> Result<BTreeMap<(String, String), Value>, String> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for (number, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |what: &str| format!("line {}: {what}", number + 1);
        if let Some(name) = line.strip_prefix('[') {
            section = name
                .strip_suffix(']')
                .ok_or_else(|| at("unclosed section header"))?
                .trim()
                .to_owned();
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| at("expected key = value"))?;
        let value = value.trim();
        let value = if let Some(items) = value.strip_prefix('[') {
            let items = items
                .strip_suffix(']')
                .ok_or_else(|| at("unclosed array"))?;
            let numbers: Result<Vec<i64>, _> = items
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::parse)
                .collect();
            Value::Array(numbers.map_err(|_| at("arrays hold whole numbers only"))?)
        } else if value == "true" || value == "false" {
            Value::Bool(value == "true")
        } else {
            Value::Integer(
                value
                    .parse()
                    .map_err(|_| at("expected a whole number or an array"))?,
            )
        };
        out.insert((section.clone(), key.trim().to_owned()), value);
    }
    Ok(out)
}

/// The file written when none exists, with the defaults filled in.
pub fn default_file() -> String {
    let defaults = Config::default();
    let table: Vec<String> = defaults
        .altars_per_flame_level
        .iter()
        .map(u8::to_string)
        .collect();
    let vanilla: Vec<String> = VANILLA_ALTARS.iter().map(u8::to_string).collect();
    format!(
        "# ensh-mods settings. Restart the server or the game after a change.\n\
         # Players need the same file as the server.\n\
         \n\
         [flame_altars]\n\
         # Altars allowed at each Flame level, from level 0; levels past the end\n\
         # use the last entry. At most {cap}: the game groups only {cap} altar zones\n\
         # into bases. Vanilla: [{vanilla}]\n\
         altars_per_flame_level = [{table}]\n\
         \n\
         [survivors]\n\
         # Being tested: a copy of each survivor per altar. A summoning staff used in\n\
         # an altar zone without a copy places a new one there; in a zone with a\n\
         # copy it moves that copy. Copies only stand in zones, one per altar.\n\
         per_altar = false\n",
        cap = ALTAR_CAP,
        vanilla = vanilla.join(", "),
        table = table.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_doubles_vanilla_up_to_the_cap() {
        let config = Config::default();
        assert_eq!(
            config.altars_per_flame_level,
            [2, 4, 8, 12, 16, 16, 16, 16, 16, 16, 16, 16]
        );
        assert_eq!(config.altars(1), 4);
        assert_eq!(config.altars(20), 16);
    }

    #[test]
    fn the_default_file_parses_back_to_the_default() {
        let (config, problems) = Config::parse(&default_file());
        assert_eq!(config, Config::default());
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn a_custom_table_is_read_and_capped() {
        let text = "[flame_altars]\naltars_per_flame_level = [1, 3, 5, 40] # comment\n";
        let (config, problems) = Config::parse(text);
        assert_eq!(config.altars_per_flame_level, [1, 3, 5, 16]);
        assert_eq!(problems.len(), 1);
        assert_eq!(config.altars(9), 16);
    }

    #[test]
    fn a_broken_file_keeps_the_defaults_and_says_where() {
        let (config, problems) = Config::parse("[flame_altars\n");
        assert_eq!(config, Config::default());
        assert_eq!(problems, ["line 1: unclosed section header"]);
        let (_, problems) = Config::parse("[flame_altars]\naltars_per_flame_level = [1, x]\n");
        assert_eq!(problems, ["line 2: arrays hold whole numbers only"]);
    }

    #[test]
    fn unknown_keys_are_reported_and_ignored() {
        let (config, problems) = Config::parse("[other]\nspeed = 3\n");
        assert_eq!(config, Config::default());
        assert_eq!(problems, ["unknown or malformed [other] speed; ignored"]);
    }

    #[test]
    fn survivors_per_altar_are_off_unless_switched_on() {
        assert!(!Config::default().survivors_per_altar);
        let (config, problems) = Config::parse(
            "[survivors]
per_altar = true
",
        );
        assert!(config.survivors_per_altar);
        assert!(problems.is_empty());
    }

    #[test]
    fn an_empty_table_keeps_the_default() {
        let (config, problems) = Config::parse("[flame_altars]\naltars_per_flame_level = []\n");
        assert_eq!(config, Config::default());
        assert_eq!(problems.len(), 1);
    }
}
