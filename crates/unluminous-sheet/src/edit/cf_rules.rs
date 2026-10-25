//! Managing the conditional format rules of a sheet: listing them in the order they apply, editing,
//! deleting and reordering them, which is what Conditional Formatting, Manage Rules does.
//!
//! A rule is named by its priority, a number that is unique on the sheet; the lowest number applies first
//! and wins a conflict. After any change the priorities are numbered again from 1 with no gaps, as Excel
//! numbers them, so list the rules again after a change rather than keeping a priority.

use super::Session;
use crate::coord::Range;
use crate::model::{CfRule, ConditionalFormat};

/// One rule with the cells it covers, as Manage Rules lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleEntry {
    pub priority: i32,
    pub ranges: Vec<Range>,
    pub rule: CfRule,
}

/// Where Move Up, Move Down and the drag in Manage Rules put a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleMove {
    /// One place nearer the top, so it applies earlier.
    Up,
    Down,
    /// To the top: the highest priority, which is where Excel puts a new rule made in the dialog.
    First,
    Last,
}

impl Session {
    /// The rules of a sheet in the order they apply, or only those that cover some of `within`.
    pub fn conditional_rules(&self, sheet: usize, within: Option<Range>) -> Vec<RuleEntry> {
        let Some(s) = self.book.sheets.get(sheet) else { return Vec::new() };
        let mut entries: Vec<RuleEntry> = s
            .conditional
            .iter()
            .flat_map(|cf| {
                cf.rules.iter().map(|r| RuleEntry {
                    priority: r.priority,
                    ranges: cf.ranges.clone(),
                    rule: r.clone(),
                })
            })
            .filter(|e| within.is_none_or(|w| e.ranges.iter().any(|r| r.intersects(&w))))
            .collect();
        entries.sort_by_key(|e| e.priority);
        entries
    }

    /// Where a rule is: the index of its format and of the rule in it.
    fn find_rule(&self, sheet: usize, priority: i32) -> Result<(usize, usize), String> {
        self.check_sheet(sheet)?;
        self.book.sheets[sheet]
            .conditional
            .iter()
            .enumerate()
            .find_map(|(i, cf)| {
                cf.rules.iter().position(|r| r.priority == priority).map(|j| (i, j))
            })
            .ok_or_else(|| format!("There is no rule with priority {priority}."))
    }

    /// Replace a rule, keeping its place in the order and the cells it covers.
    pub fn edit_conditional_rule(
        &mut self,
        sheet: usize,
        priority: i32,
        mut rule: CfRule,
    ) -> Result<(), String> {
        let (i, j) = self.find_rule(sheet, priority)?;
        self.remember_whole("Edit rule");
        rule.priority = priority;
        // The twin in the extension is the rule's own to pair; an edited rule keeps its id.
        if rule.x14.is_none() {
            rule.x14 = self.book.sheets[sheet].conditional[i].rules[j].x14.clone();
        }
        self.book.sheets[sheet].conditional[i].rules[j] = rule;
        Ok(())
    }

    /// Change the cells a rule applies to ("Applies to").
    pub fn set_conditional_ranges(
        &mut self,
        sheet: usize,
        priority: i32,
        ranges: Vec<Range>,
    ) -> Result<(), String> {
        let (i, j) = self.find_rule(sheet, priority)?;
        if ranges.is_empty() {
            return Err("A rule needs at least one range to apply to.".to_owned());
        }
        self.remember_whole("Edit rule");
        let s = &mut self.book.sheets[sheet];
        let rule = s.conditional[i].rules.remove(j);
        if s.conditional[i].rules.is_empty() {
            s.conditional.remove(i);
        }
        s.conditional.push(ConditionalFormat { ranges, rules: vec![rule] });
        Ok(())
    }

    /// Turn Stop If True on or off for a rule.
    pub fn set_conditional_stop(
        &mut self,
        sheet: usize,
        priority: i32,
        stop: bool,
    ) -> Result<(), String> {
        let (i, j) = self.find_rule(sheet, priority)?;
        self.remember_whole("Edit rule");
        self.book.sheets[sheet].conditional[i].rules[j].stop = stop;
        Ok(())
    }

    /// Delete one rule.
    pub fn delete_conditional_rule(&mut self, sheet: usize, priority: i32) -> Result<(), String> {
        let (i, j) = self.find_rule(sheet, priority)?;
        self.remember_whole("Delete rule");
        let s = &mut self.book.sheets[sheet];
        s.conditional[i].rules.remove(j);
        s.conditional.retain(|cf| !cf.rules.is_empty());
        self.renumber_conditional(sheet);
        Ok(())
    }

    /// Move a rule up or down the order, or to either end of it.
    pub fn move_conditional_rule(
        &mut self,
        sheet: usize,
        priority: i32,
        movement: RuleMove,
    ) -> Result<(), String> {
        self.find_rule(sheet, priority)?;
        let order: Vec<i32> =
            self.conditional_rules(sheet, None).iter().map(|e| e.priority).collect();
        let at = order.iter().position(|p| *p == priority).unwrap_or(0);
        let target = match movement {
            RuleMove::Up => at.saturating_sub(1),
            RuleMove::Down => (at + 1).min(order.len() - 1),
            RuleMove::First => 0,
            RuleMove::Last => order.len() - 1,
        };
        if target == at {
            return Ok(());
        }
        self.remember_whole("Reorder rules");
        let mut new_order = order;
        let moved = new_order.remove(at);
        new_order.insert(target, moved);
        // Give each rule the number of its new place: the rule that had priority `p` is now `rank + 1`.
        let renumber: Vec<(i32, i32)> =
            new_order.iter().enumerate().map(|(rank, p)| (*p, rank as i32 + 1)).collect();
        for cf in &mut self.book.sheets[sheet].conditional {
            for rule in &mut cf.rules {
                if let Some((_, n)) = renumber.iter().find(|(old, _)| *old == rule.priority) {
                    rule.priority = *n;
                }
            }
        }
        Ok(())
    }

    /// Number the rules of a sheet from 1 in the order they apply.
    fn renumber_conditional(&mut self, sheet: usize) {
        let mut all: Vec<(i32, usize, usize)> = Vec::new();
        for (i, cf) in self.book.sheets[sheet].conditional.iter().enumerate() {
            for (j, r) in cf.rules.iter().enumerate() {
                all.push((r.priority, i, j));
            }
        }
        all.sort_unstable();
        for (rank, (_, i, j)) in all.into_iter().enumerate() {
            self.book.sheets[sheet].conditional[i].rules[j].priority = rank as i32 + 1;
        }
    }
}
