//! `croma agent` — render the notation help topics for the terminal.
//!
//! The topic data lives in `croma-core` ([`croma_core::agent_topics`]) so a
//! library user gets the same knowledge without the CLI; this module only adds
//! the text presentation (the index and per-topic pages).

use croma_core::AgentTopic;

/// The index printed by a bare `croma agent`: every topic, grouped by category
/// in database order, with a usage hint.
pub(crate) fn render_index(topics: &[AgentTopic]) -> String {
    let mut out = String::from(
        "croma special ABC notations — help topics\n\n\
         Run `croma agent <topic>` for the syntax and a copy-paste example.\n\
         These `[I:cr <code> ...]` / `[I:croma-*]` / `%%croma-*` annotations carry\n\
         MusicXML facts that plain ABC cannot express; other ABC tools ignore them.\n",
    );
    let mut current = "";
    for topic in topics {
        if topic.category != current {
            out.push_str(&format!("\n{}\n", topic.category));
            current = topic.category;
        }
        out.push_str(&format!("  {:<22} {}\n", topic.id, topic.summary));
    }
    out
}

/// The page printed by `croma agent <topic>`.
pub(crate) fn render_topic(topic: &AgentTopic) -> String {
    let mut out = format!("# {}\n", topic.id);
    if !topic.aliases.is_empty() {
        out.push_str(&format!("aliases: {}\n", topic.aliases.join(", ")));
    }
    out.push_str(&format!("category: {}\n\n", topic.category));
    out.push_str(topic.summary);
    out.push_str("\n\n");
    out.push_str(topic.body.trim_end());
    out.push('\n');
    out
}

/// Comma-separated topic ids, for the unknown-topic error.
pub(crate) fn topic_ids(topics: &[AgentTopic]) -> String {
    topics
        .iter()
        .map(|topic| topic.id)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
