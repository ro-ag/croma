use super::*;

#[test]
fn topics_are_nonempty_and_have_a_syntax_overview() {
    assert!(!agent_topics().is_empty());
    assert_eq!(find_agent_topic("syntax").map(|t| t.id), Some("syntax"));
}

#[test]
fn find_matches_id_and_alias_case_insensitively() {
    let by_alias =
        find_agent_topic("CROMA-XVOICE-SLUR").expect("the croma-xvoice-slur alias must resolve");
    assert_eq!(by_alias.id, "xvoice-slur");
    assert!(find_agent_topic("no-such-topic").is_none());
}

/// The `%%score` grouping topic (issue #259): reachable by every alias an
/// agent is likely to try, and its body states the three grouping semantics.
#[test]
fn score_topic_documents_staff_grouping() {
    for query in ["score", "%%score", "grouping", "staves", "%%staves"] {
        assert_eq!(
            find_agent_topic(query).map(|t| t.id),
            Some("score"),
            "`{query}` must resolve to the score topic"
        );
    }
    let topic = find_agent_topic("score").expect("the score topic must exist");
    assert!(topic.body.contains("ONE staff"), "( ) overlay semantics");
    assert!(topic.body.contains("grand staff"), "{{ }} semantics");
    assert!(topic.body.contains("%%score Lead"), "runnable example");
}

#[test]
fn carrier_topics_show_their_inline_or_header_syntax() {
    for topic in agent_topics() {
        if topic.aliases.iter().any(|a| a.starts_with("croma-")) {
            assert!(
                topic.body.contains("[I:croma-") || topic.body.contains("%%croma-"),
                "topic `{}` must show a copy-paste carrier example",
                topic.id
            );
        }
    }
}

/// Anti-drift: every carrier in `docs/carriers.md` must have a topic, so
/// adding a carrier forces a topic here.
#[test]
fn every_carrier_in_the_spec_has_a_topic() {
    let spec = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/carriers.md"
    ))
    .expect("docs/carriers.md must be readable");
    let carriers: Vec<String> = spec
        .lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("| `croma-")?;
            Some(format!("croma-{}", rest.split('`').next()?))
        })
        .collect();
    assert!(
        carriers.len() >= 20,
        "expected catalogue carriers; got {carriers:?}"
    );

    let haystack = agent_topics()
        .iter()
        .map(|t| format!("{} {} {}", t.id, t.aliases.join(" "), t.body))
        .collect::<Vec<_>>()
        .join("\n");
    for carrier in carriers {
        assert!(
            haystack.contains(&carrier),
            "carrier `{carrier}` from docs/carriers.md has no agent topic"
        );
    }
}
