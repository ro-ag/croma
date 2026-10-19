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
