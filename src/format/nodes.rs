//! Node-kind dispatch for the bash reformatter: [`Formatter::format_node`]
//! plus the leaf-construct helpers (`format_command`,
//! `format_command_words`, `format_cond_node`) that don't warrant a
//! dedicated compound-construct file.

use crate::ast::{Node, NodeKind};

use super::formatter::Formatter;
use super::words::process_word_value;

impl Formatter {
    /// Format a single AST node into canonical bash source. Top-level
    /// dispatch for the reformatter — delegates each node kind to the
    /// dedicated helper method (most live in `compound.rs`).
    ///
    /// Simple leaf kinds are handled inline; everything else is routed
    /// through [`Self::format_compound_node`] so each match body stays
    /// under the clippy `too_many_lines` threshold.
    pub(super) fn format_node(&mut self, node: &Node) {
        match &node.kind {
            NodeKind::Word { value, .. } => self.write_str(value),
            NodeKind::Pipeline { commands, .. } => self.format_pipeline(commands),
            NodeKind::List { items } => self.format_list(items),
            NodeKind::Empty => {}
            _ => self.format_compound_node(node),
        }
    }

    /// Dispatch for compound and leaf-with-fields node kinds. Each arm
    /// delegates to a helper taking `&Node`; the helper destructures
    /// via `let-else` internally. This keeps the dispatcher flat so
    /// the function stays under 60 lines.
    fn format_compound_node(&mut self, node: &Node) {
        match &node.kind {
            NodeKind::Command { .. } => self.format_command(node),
            NodeKind::If { .. } => self.format_if(node),
            NodeKind::While { .. } => self.format_while_until(node, "while"),
            NodeKind::Until { .. } => self.format_while_until(node, "until"),
            NodeKind::For { .. } => self.format_for(node),
            NodeKind::Select { .. } => self.format_select(node),
            NodeKind::ForArith { .. } => self.format_for_arith(node),
            NodeKind::Case { .. } => self.format_case(node),
            NodeKind::Function { .. } => self.format_function(node),
            NodeKind::Subshell { .. } => self.format_subshell(node),
            NodeKind::BraceGroup { .. } => self.format_brace_group(node),
            NodeKind::Negation { .. } => self.format_negation(node),
            NodeKind::Time { .. } => self.format_time(node),
            NodeKind::Coproc { .. } => self.format_coproc(node),
            NodeKind::ConditionalExpr { .. } => self.format_conditional_expr(node),
            _ => self.write_str(&node.to_string()),
        }
    }

    pub(super) fn format_command(&mut self, node: &Node) {
        let NodeKind::Command {
            assignments,
            words,
            redirects,
        } = &node.kind
        else {
            return;
        };
        self.format_command_words(assignments, words);
        // Phase 1: all redirect heads inline on the command line.
        // For heredocs this is just the `<<DELIM` part — the bodies are
        // deferred so bash's canonical form can put every op on the same
        // line regardless of heredoc ordering (see #39).
        for (i, r) in redirects.iter().enumerate() {
            if !assignments.is_empty() || !words.is_empty() || i > 0 {
                self.write_char(' ');
            }
            self.format_redirect_inline(r);
        }
        // Phase 2: concatenate heredoc bodies in source order after the
        // command line. Exactly one `\n` separates the command line from
        // the first body; consecutive bodies have none between them
        // (each body already ends with its delimiter's trailing `\n`).
        let mut first_heredoc = true;
        for r in redirects {
            if let NodeKind::HereDoc {
                delimiter, content, ..
            } = &r.kind
            {
                if first_heredoc {
                    self.write_char('\n');
                    first_heredoc = false;
                }
                self.write_heredoc_body(content, delimiter);
            }
        }
    }

    /// Writes assignments and command words as space-separated bash tokens.
    pub(super) fn format_command_words(&mut self, assignments: &[Node], words: &[Node]) {
        for (i, w) in assignments.iter().chain(words.iter()).enumerate() {
            if i > 0 {
                self.write_char(' ');
            }
            if let NodeKind::Word { value, spans, .. } = &w.kind {
                self.write_str(&process_word_value(value, spans));
            } else {
                self.write_str(&w.to_string());
            }
        }
    }

    /// Formats a conditional expression node as canonical bash source.
    /// Callers place the `[[ ` / ` ]]` delimiters; this helper emits
    /// only the inner expression.
    pub(super) fn format_cond_node(&mut self, node: &Node) {
        match &node.kind {
            NodeKind::UnaryTest { op, operand } => {
                self.write_str(op);
                self.write_char(' ');
                self.format_cond_node(operand);
            }
            NodeKind::BinaryTest { op, left, right } => {
                self.format_cond_node(left);
                self.write_char(' ');
                self.write_str(op);
                self.write_char(' ');
                self.format_cond_node(right);
            }
            NodeKind::CondAnd { left, right } => {
                self.format_cond_node(left);
                self.write_str(" && ");
                self.format_cond_node(right);
            }
            NodeKind::CondOr { left, right } => {
                self.format_cond_node(left);
                self.write_str(" || ");
                self.format_cond_node(right);
            }
            NodeKind::CondNot { operand } => {
                self.write_str("! ");
                self.format_cond_node(operand);
            }
            NodeKind::CondTerm { value, .. } => {
                self.write_str(value);
            }
            NodeKind::CondParen { inner } => {
                self.write_str("( ");
                self.format_cond_node(inner);
                self.write_str(" )");
            }
            _ => {
                self.write_str(&node.to_string());
            }
        }
    }
}
