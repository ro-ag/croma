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
         These `[I:croma-*]` / `%%croma-*` annotations carry MusicXML facts that\n\
         plain ABC cannot express; other ABC tools ignore them.\n",
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
mod tests {
    use super::*;
    use croma_core::agent_topics;

    #[test]
    fn index_lists_every_topic_with_a_usage_hint() {
        let index = render_index(agent_topics());
        assert!(
            index.contains("croma agent <topic>"),
            "index must hint usage"
        );
        for topic in agent_topics() {
            assert!(
                index.contains(topic.id),
                "index must list topic `{}`",
                topic.id
            );
        }
    }

    #[test]
    fn topic_page_shows_summary_and_body() {
        let topic = croma_core::find_agent_topic("xvoice-slur").expect("topic exists");
        let page = render_topic(topic);
        assert!(page.contains("# xvoice-slur"));
        assert!(page.contains("[I:croma-xvoice-slur"));
        assert!(page.contains("aliases: croma-xvoice-slur"));
    }
}
