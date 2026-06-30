//! `croma agent` — help topics that explain croma's non-standard ABC notations
//! (the `[I:croma-*]` / `%%croma-*` carriers) to an AI agent or LLM, so it can
//! author ABC annotations that persist through to MusicXML.
//!
//! The topic database is a JSON file embedded at compile time (Rust's
//! equivalent of Go's `embed`): [`TOPICS_JSON`]. [`docs/carriers.md`] stays the
//! canonical spec; these topics are its agent-facing distillation, and a test
//! asserts every carrier in that spec has a topic so the two cannot drift.

use serde::Deserialize;

/// The topic database, embedded at compile time.
const TOPICS_JSON: &str = include_str!("agent_topics.json");

/// One help topic: a croma notation (or the `syntax` overview), explained for an
/// agent with a copy-paste ABC example.
#[derive(Debug, Deserialize)]
pub(crate) struct Topic {
    /// Short, goal-oriented id (`xvoice-slur`, `lyrics`), the lookup key.
    pub id: String,
    /// Synonyms — the full `croma-<name>` carrier name and common phrasings.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Catalogue group, mirroring `docs/carriers.md` (`Basics`, `Lyrics`, …).
    pub category: String,
    /// One-line description shown in the index.
    pub summary: String,
    /// The full agent-facing explanation (Markdown): what MusicXML fact it
    /// persists, the syntax, and a runnable ABC example.
    pub body: String,
}

/// Parse the embedded topic database. Panics only if the bundled JSON is
/// malformed — a build-time authoring error caught by the unit tests.
pub(crate) fn load_topics() -> Vec<Topic> {
    serde_json::from_str(TOPICS_JSON).expect("embedded agent_topics.json must be valid JSON")
}

/// Find a topic by `id` or any alias, case-insensitively.
pub(crate) fn find<'a>(topics: &'a [Topic], query: &str) -> Option<&'a Topic> {
    let query = query.trim().to_ascii_lowercase();
    topics.iter().find(|topic| {
        topic.id.eq_ignore_ascii_case(&query)
            || topic
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&query))
    })
}

/// The index printed by a bare `croma agent`: every topic, grouped by category
/// in database order, with a usage hint.
pub(crate) fn render_index(topics: &[Topic]) -> String {
    let mut out = String::from(
        "croma special ABC notations — help topics\n\n\
         Run `croma agent <topic>` for the syntax and a copy-paste example.\n\
         These `[I:croma-*]` / `%%croma-*` annotations carry MusicXML facts that\n\
         plain ABC cannot express; other ABC tools ignore them.\n",
    );
    let mut current = "";
    for topic in topics {
        if topic.category != current {
            out.push_str(&format!("\n{}\n", topic.category));
            current = &topic.category;
        }
        out.push_str(&format!("  {:<22} {}\n", topic.id, topic.summary));
    }
    out
}

/// The page printed by `croma agent <topic>`.
pub(crate) fn render_topic(topic: &Topic) -> String {
    let mut out = format!("# {}\n", topic.id);
    if !topic.aliases.is_empty() {
        out.push_str(&format!("aliases: {}\n", topic.aliases.join(", ")));
    }
    out.push_str(&format!("category: {}\n\n", topic.category));
    out.push_str(&topic.summary);
    out.push_str("\n\n");
    out.push_str(topic.body.trim_end());
    out.push('\n');
    out
}

/// Comma-separated topic ids, for the unknown-topic error.
pub(crate) fn topic_ids(topics: &[Topic]) -> String {
    topics
        .iter()
        .map(|topic| topic.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_database_parses_and_is_nonempty() {
        let topics = load_topics();
        assert!(
            !topics.is_empty(),
            "the agent topic database must not be empty"
        );
    }

    #[test]
    fn index_lists_every_topic_with_a_usage_hint() {
        let topics = load_topics();
        let index = render_index(&topics);
        assert!(
            index.contains("croma agent <topic>"),
            "index must hint usage"
        );
        for topic in &topics {
            assert!(
                index.contains(&topic.id),
                "index must list topic `{}`",
                topic.id
            );
        }
    }

    #[test]
    fn find_matches_id_and_alias_case_insensitively() {
        let topics = load_topics();
        let syntax = find(&topics, "syntax").expect("a `syntax` topic must exist");
        assert_eq!(syntax.id, "syntax");
        // Every carrier topic is reachable by its full `croma-<name>` alias.
        let by_alias = find(&topics, "CROMA-XVOICE-SLUR")
            .expect("a topic must answer to the `croma-xvoice-slur` alias");
        assert!(by_alias.aliases.iter().any(|a| a == "croma-xvoice-slur"));
        assert!(find(&topics, "no-such-topic").is_none());
    }

    #[test]
    fn carrier_topics_show_their_inline_or_header_syntax() {
        let topics = load_topics();
        for topic in &topics {
            // Every carrier topic (one whose alias is a `croma-*` name) must show
            // a concrete `[I:croma-*]` or `%%croma-*` form an agent can copy.
            if topic.aliases.iter().any(|a| a.starts_with("croma-")) {
                assert!(
                    topic.body.contains("[I:croma-") || topic.body.contains("%%croma-"),
                    "topic `{}` must show a copy-paste carrier example",
                    topic.id
                );
            }
        }
    }

    /// Anti-drift: every carrier documented in `docs/carriers.md` must have a
    /// topic, so adding a carrier forces a topic here.
    #[test]
    fn every_carrier_in_the_spec_has_a_topic() {
        let spec = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/carriers.md"
        ))
        .expect("docs/carriers.md must be readable");

        // Catalogue rows look like: `| \`croma-<name>\` | … |`. Pull the carrier
        // name from each such row (skips prose mentions like the `croma-fmt`
        // "Not a carrier" note, which is not a table row).
        let carriers: Vec<String> = spec
            .lines()
            .filter_map(|line| {
                let rest = line.trim_start().strip_prefix("| `croma-")?;
                let name = rest.split('`').next()?;
                Some(format!("croma-{name}"))
            })
            .collect();
        assert!(
            carriers.len() >= 20,
            "expected the carriers.md catalogue to yield carriers; got {carriers:?}"
        );

        let topics = load_topics();
        let haystack = topics
            .iter()
            .map(|topic| format!("{} {} {}", topic.id, topic.aliases.join(" "), topic.body))
            .collect::<Vec<_>>()
            .join("\n");
        for carrier in carriers {
            assert!(
                haystack.contains(&carrier),
                "carrier `{carrier}` from docs/carriers.md has no `croma agent` topic"
            );
        }
    }
}
