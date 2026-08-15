//! Local rules engine and Sieve converter for mail filtering and routing (Roadmap 3.6).

use crate::types::{
    MailRule, MessageDetail, MessageRow, RuleAction, RuleCondition, RuleField, RuleMatchMode,
    RuleOperator,
};

/// Evaluates a single rule condition against message row and optional detail.
pub fn evaluate_condition(
    cond: &RuleCondition,
    row: &MessageRow,
    detail: Option<&MessageDetail>,
) -> bool {
    match cond.field {
        RuleField::From => {
            let val = &cond.value;
            check_text_match(&cond.operator, &row.sender_address, val)
                || check_text_match(&cond.operator, &row.sender_name, val)
        }
        RuleField::To => {
            let val = &cond.value;
            if let Some(d) = detail {
                d.to.iter().any(|r| {
                    check_text_match(&cond.operator, &r.address, val)
                        || check_text_match(&cond.operator, &r.name, val)
                })
            } else {
                // If detail is not loaded, check sender/snippet or treat as false
                false
            }
        }
        RuleField::Cc => {
            let val = &cond.value;
            if let Some(d) = detail {
                d.cc.iter().any(|r| {
                    check_text_match(&cond.operator, &r.address, val)
                        || check_text_match(&cond.operator, &r.name, val)
                })
            } else {
                false
            }
        }
        RuleField::Subject => check_text_match(&cond.operator, &row.subject, &cond.value),
        RuleField::ListId => {
            let val = &cond.value;
            if let Some(d) = detail {
                if let Some(ref mid) = d.message_id_header {
                    if check_text_match(&cond.operator, mid, val) {
                        return true;
                    }
                }
                if let Some(ref refs) = d.references {
                    if check_text_match(&cond.operator, refs, val) {
                        return true;
                    }
                }
            }
            check_text_match(&cond.operator, &row.subject, val)
        }
        RuleField::HasAttachment => {
            let has = row.has_attachments
                || detail.map_or(false, |d| !d.attachments.is_empty());
            let want = cond.value.trim().eq_ignore_ascii_case("true")
                || cond.value.trim().eq_ignore_ascii_case("yes")
                || cond.value.trim() == "1";
            match cond.operator {
                RuleOperator::Equals | RuleOperator::Contains => has == want,
                RuleOperator::NotEquals => has != want,
                _ => has,
            }
        }
        RuleField::Body => {
            let val = &cond.value;
            if check_text_match(&cond.operator, &row.snippet, val) {
                return true;
            }
            if let Some(d) = detail {
                let full_body = d.body.join(" ");
                check_text_match(&cond.operator, &full_body, val)
            } else {
                false
            }
        }
    }
}

fn check_text_match(op: &RuleOperator, text: &str, pattern: &str) -> bool {
    let t = text.to_lowercase();
    let p = pattern.to_lowercase();
    match op {
        RuleOperator::Contains => t.contains(&p),
        RuleOperator::NotContains => !t.contains(&p),
        RuleOperator::Equals => t == p,
        RuleOperator::NotEquals => t != p,
        RuleOperator::StartsWith => t.starts_with(&p),
        RuleOperator::EndsWith => t.ends_with(&p),
        RuleOperator::Matches => wildcard_match(&t, &p),
    }
}

fn wildcard_match(text: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }
    let mut current = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !current.starts_with(part) {
                return false;
            }
            current = &current[part.len()..];
        } else if i == parts.len() - 1 {
            return current.ends_with(part);
        } else if let Some(pos) = current.find(part) {
            current = &current[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Evaluates whether a single rule matches a message.
pub fn matches_rule(
    rule: &MailRule,
    message: &MessageRow,
    detail: Option<&MessageDetail>,
) -> bool {
    if !rule.enabled || rule.conditions.is_empty() {
        return false;
    }

    match rule.match_mode {
        RuleMatchMode::All => rule
            .conditions
            .iter()
            .all(|c| evaluate_condition(c, message, detail)),
        RuleMatchMode::Any => rule
            .conditions
            .iter()
            .any(|c| evaluate_condition(c, message, detail)),
    }
}

/// The indices of the rules that match a message, in evaluation order,
/// honoring `stop_processing` — the dry-run's ordering explanation (P1.3).
pub fn matching_rules(
    rules: &[MailRule],
    message: &MessageRow,
    detail: Option<&MessageDetail>,
) -> Vec<usize> {
    let mut matched = Vec::new();
    for (i, rule) in rules.iter().enumerate() {
        if matches_rule(rule, message, detail) {
            matched.push(i);
            if rule.stop_processing {
                break;
            }
        }
    }
    matched
}

/// Evaluates an ordered slice of rules against a message, accumulating actions
/// until either all rules are processed or a matching rule specifies `stop_processing`.
pub fn evaluate_rules(
    rules: &[MailRule],
    message: &MessageRow,
    detail: Option<&MessageDetail>,
) -> Vec<RuleAction> {
    let mut actions = Vec::new();
    for i in matching_rules(rules, message, detail) {
        actions.extend(rules[i].actions.clone());
    }
    actions
}

/// Converts a slice of Quill `MailRule`s to RFC 5228 Sieve script text.
pub fn export_sieve(rules: &[MailRule]) -> String {
    let mut out = String::new();
    out.push_str("# Generated by Quill Mail (RFC 5228 Sieve)\n");
    out.push_str("require [\"fileinto\", \"reject\"];\n\n");

    for (i, rule) in rules.iter().enumerate() {
        let kw = if i == 0 { "if" } else { "elsif" };
        let cond_mode = match rule.match_mode {
            RuleMatchMode::All => "allof",
            RuleMatchMode::Any => "anyof",
        };

        let mut cond_strs = Vec::new();
        for cond in &rule.conditions {
            let field_header = match cond.field {
                RuleField::From => "from",
                RuleField::To => "to",
                RuleField::Cc => "cc",
                RuleField::Subject => "subject",
                RuleField::ListId => "list-id",
                RuleField::HasAttachment => "has-attachment",
                RuleField::Body => "body",
            };
            let match_type = match cond.operator {
                RuleOperator::Contains => ":contains",
                RuleOperator::NotContains => ":not_contains",
                RuleOperator::Equals => ":is",
                RuleOperator::NotEquals => ":not_is",
                RuleOperator::StartsWith => ":startswith",
                RuleOperator::EndsWith => ":endswith",
                RuleOperator::Matches => ":matches",
            };
            cond_strs.push(format!(
                "header {} \"{}\" \"{}\"",
                match_type, field_header, cond.value
            ));
        }

        let cond_expr = if cond_strs.len() == 1 {
            cond_strs[0].clone()
        } else {
            format!("{} (\n    {}\n)", cond_mode, cond_strs.join(",\n    "))
        };

        out.push_str(&format!("# Rule: {}\n{} {} {{\n", rule.name, kw, cond_expr));

        for action in &rule.actions {
            match action {
                RuleAction::MoveToFolder { folder_name } => {
                    out.push_str(&format!("    fileinto \"{}\";\n", folder_name));
                }
                RuleAction::MarkRead => {
                    out.push_str("    setflag \"\\\\Seen\";\n");
                }
                RuleAction::MarkUnread => {
                    out.push_str("    removeflag \"\\\\Seen\";\n");
                }
                RuleAction::MarkFlagged => {
                    out.push_str("    setflag \"\\\\Flagged\";\n");
                }
                RuleAction::MarkUnflagged => {
                    out.push_str("    removeflag \"\\\\Flagged\";\n");
                }
                RuleAction::Delete => {
                    out.push_str("    discard;\n");
                }
                RuleAction::Archive => {
                    out.push_str("    fileinto \"Archive\";\n");
                }
            }
        }

        if rule.stop_processing {
            out.push_str("    stop;\n");
        }

        out.push_str("}\n\n");
    }

    out
}

/// Parses a simple RFC 5228 Sieve script into a list of Quill `MailRule`s.
pub fn parse_sieve(script: &str) -> Result<Vec<MailRule>, String> {
    let mut rules = Vec::new();
    let mut rule_counter = 1;

    // Normalize and clean comments / whitespace
    let lines: Vec<&str> = script.lines().collect();
    let mut current_name = String::new();
    let mut in_block = false;
    let mut block_lines: Vec<String> = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if let Some(name_part) = trimmed.strip_prefix("# Rule:") {
                current_name = name_part.trim().to_string();
            } else if let Some(name_part) = trimmed.strip_prefix('#') {
                if current_name.is_empty() && !name_part.trim().is_empty() {
                    current_name = name_part.trim().to_string();
                }
            }
            continue;
        }

        if trimmed.starts_with("require") {
            continue;
        }

        if (trimmed.starts_with("if ") || trimmed.starts_with("elsif ")) && trimmed.contains('{') {
            in_block = true;
            block_lines.clear();
            block_lines.push(trimmed.to_string());
            if trimmed.ends_with('}') {
                in_block = false;
                if let Some(r) = parse_sieve_block(&block_lines, &current_name, rule_counter) {
                    rules.push(r);
                    rule_counter += 1;
                    current_name.clear();
                }
            }
            continue;
        }

        if in_block {
            block_lines.push(trimmed.to_string());
            if trimmed == "}" || trimmed.ends_with('}') {
                in_block = false;
                if let Some(r) = parse_sieve_block(&block_lines, &current_name, rule_counter) {
                    rules.push(r);
                    rule_counter += 1;
                    current_name.clear();
                }
            }
        }
    }

    Ok(rules)
}

fn parse_sieve_block(
    lines: &[String],
    custom_name: &str,
    rule_index: usize,
) -> Option<MailRule> {
    if lines.is_empty() {
        return None;
    }

    let full_text = lines.join(" ");
    let open_idx = full_text.find('{')?;
    let cond_header = &full_text[..open_idx];
    let body_text = &full_text[open_idx + 1..full_text.rfind('}').unwrap_or(full_text.len())];

    let mut cond_text = cond_header.trim();
    if let Some(rest) = cond_text.strip_prefix("elsif") {
        cond_text = rest.trim();
    } else if let Some(rest) = cond_text.strip_prefix("if") {
        cond_text = rest.trim();
    }

    let match_mode = if cond_text.contains("anyof") {
        RuleMatchMode::Any
    } else {
        RuleMatchMode::All
    };

    let mut conditions = Vec::new();

    // Look for header matches in cond_text
    for part in cond_text.split(|c| c == ',' || c == '(' || c == ')') {
        let trimmed = part.trim();
        if trimmed.is_empty() || trimmed == "allof" || trimmed == "anyof" {
            continue;
        }

        let mut field = RuleField::From;
        let mut op = RuleOperator::Contains;

        if trimmed.contains(":contains") {
            op = RuleOperator::Contains;
        } else if trimmed.contains(":is") {
            op = RuleOperator::Equals;
        } else if trimmed.contains(":startswith") {
            op = RuleOperator::StartsWith;
        } else if trimmed.contains(":endswith") {
            op = RuleOperator::EndsWith;
        } else if trimmed.contains(":matches") {
            op = RuleOperator::Matches;
        }

        let quotes: Vec<&str> = trimmed.split('"').collect();
        if quotes.len() >= 4 {
            let f = quotes[1].to_lowercase();
            if f.contains("from") {
                field = RuleField::From;
            } else if f.contains("to") {
                field = RuleField::To;
            } else if f.contains("cc") {
                field = RuleField::Cc;
            } else if f.contains("subject") {
                field = RuleField::Subject;
            } else if f.contains("list") {
                field = RuleField::ListId;
            } else if f.contains("body") {
                field = RuleField::Body;
            }
            let val = quotes[3].to_string();
            conditions.push(RuleCondition {
                field,
                operator: op,
                value: val,
            });
        }
    }

    if conditions.is_empty() {
        // Fallback default condition
        conditions.push(RuleCondition {
            field: RuleField::Subject,
            operator: RuleOperator::Contains,
            value: "".to_string(),
        });
    }

    let mut actions = Vec::new();
    let mut stop_processing = false;

    for stmt in body_text.split(';') {
        let stmt_trim = stmt.trim();
        if stmt_trim.starts_with("fileinto") {
            let q: Vec<&str> = stmt_trim.split('"').collect();
            if q.len() >= 2 {
                actions.push(RuleAction::MoveToFolder {
                    folder_name: q[1].to_string(),
                });
            }
        } else if stmt_trim.starts_with("discard") {
            actions.push(RuleAction::Delete);
        } else if stmt_trim.contains("\\Seen") {
            actions.push(RuleAction::MarkRead);
        } else if stmt_trim.contains("\\Flagged") {
            actions.push(RuleAction::MarkFlagged);
        } else if stmt_trim == "stop" {
            stop_processing = true;
        }
    }

    let name = if !custom_name.is_empty() {
        custom_name.to_string()
    } else {
        format!("Rule {}", rule_index)
    };

    Some(MailRule {
        id: format!("rule_{}", rule_index),
        name,
        enabled: true,
        match_mode,
        conditions,
        actions,
        stop_processing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row(sender: &str, subject: &str) -> MessageRow {
        MessageRow {
            id: 1,
            account_id: 1,
            folder: "Inbox".into(),
            sender_name: "Boss".into(),
            sender_address: sender.into(),
            subject: subject.into(),
            snippet: "Please see attached quarterly review".into(),
            received_at_ms: 1700000000000,
            unread: true,
            flagged: false,
            answered: false,
            forwarded: false,
            thread_count: 1,
            thread_id: None,
            has_attachments: true,
        }
    }

    #[test]
    fn test_condition_evaluation() {
        let row = sample_row("ceo@company.com", "[Urgent] Quarterly report");
        let cond1 = RuleCondition {
            field: RuleField::From,
            operator: RuleOperator::Contains,
            value: "company.com".into(),
        };
        assert!(evaluate_condition(&cond1, &row, None));

        let cond2 = RuleCondition {
            field: RuleField::Subject,
            operator: RuleOperator::StartsWith,
            value: "[Urgent]".into(),
        };
        assert!(evaluate_condition(&cond2, &row, None));

        let cond3 = RuleCondition {
            field: RuleField::HasAttachment,
            operator: RuleOperator::Equals,
            value: "true".into(),
        };
        assert!(evaluate_condition(&cond3, &row, None));
    }

    #[test]
    fn test_rules_evaluation_and_stop_processing() {
        let row = sample_row("alerts@service.com", "Server down alert");
        let rules = vec![
            MailRule {
                id: "1".into(),
                name: "Alerts".into(),
                enabled: true,
                match_mode: RuleMatchMode::All,
                conditions: vec![RuleCondition {
                    field: RuleField::From,
                    operator: RuleOperator::Contains,
                    value: "alerts@".into(),
                }],
                actions: vec![
                    RuleAction::MoveToFolder {
                        folder_name: "Alerts".into(),
                    },
                    RuleAction::MarkFlagged,
                ],
                stop_processing: true,
            },
            MailRule {
                id: "2".into(),
                name: "Never hit".into(),
                enabled: true,
                match_mode: RuleMatchMode::All,
                conditions: vec![RuleCondition {
                    field: RuleField::Subject,
                    operator: RuleOperator::Contains,
                    value: "Server".into(),
                }],
                actions: vec![RuleAction::MarkRead],
                stop_processing: false,
            },
        ];

        let actions = evaluate_rules(&rules, &row, None);
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0],
            RuleAction::MoveToFolder {
                folder_name: "Alerts".into()
            }
        );
        assert_eq!(actions[1], RuleAction::MarkFlagged);
    }

    #[test]
    fn test_sieve_export_and_import_roundtrip() {
        let rules = vec![MailRule {
            id: "rule_1".into(),
            name: "Work Mails".into(),
            enabled: true,
            match_mode: RuleMatchMode::All,
            conditions: vec![RuleCondition {
                field: RuleField::From,
                operator: RuleOperator::Contains,
                value: "boss@work.com".into(),
            }],
            actions: vec![
                RuleAction::MoveToFolder {
                    folder_name: "Work".into(),
                },
                RuleAction::MarkFlagged,
            ],
            stop_processing: true,
        }];

        let sieve = export_sieve(&rules);
        assert!(sieve.contains("require [\"fileinto\", \"reject\"];"));
        assert!(sieve.contains("header :contains \"from\" \"boss@work.com\""));
        assert!(sieve.contains("fileinto \"Work\";"));
        assert!(sieve.contains("stop;"));

        let parsed = parse_sieve(&sieve).expect("parse sieve failed");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Work Mails");
        assert_eq!(parsed[0].conditions.len(), 1);
        assert_eq!(parsed[0].conditions[0].value, "boss@work.com");
        assert_eq!(parsed[0].actions.len(), 2);
        assert!(parsed[0].stop_processing);
    }
}
