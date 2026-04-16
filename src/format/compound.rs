//! Compound-construct formatters for the reformatter: if / while / until
//! / for / select / case / function / subshell / brace-group / negation
//! / time / coproc / `[[…]]` conditional expressions.
//!
//! Each entry here is an `impl Formatter` method on
//! [`super::formatter::Formatter`]. The top-level dispatch in
//! `nodes.rs::Formatter::format_node` delegates to these.
//!
//! Helpers take `&Node` and destructure internally via `let-else`, so
//! the dispatcher can be a flat `NodeKind::X { .. } => self.format_x(node)`
//! match without repeating the field list in both places. The `let-else`
//! early-returns on a mismatched variant — unreachable in practice
//! because the dispatcher only calls the matching helper, but preserves
//! total functions (no `panic!` or `unreachable!`).

use crate::ast::{Node, NodeKind};

use super::formatter::Formatter;

impl Formatter {
    pub(super) fn format_if(&mut self, node: &Node) {
        let NodeKind::If {
            condition,
            then_body,
            else_body,
            ..
        } = &node.kind
        else {
            return;
        };
        self.write_str("if ");
        self.format_node(condition);
        self.write_str("; then\n");
        self.with_indent(4, |f| {
            f.indent_here();
            f.format_node(then_body);
            f.write_str(";\n");
        });
        if let Some(eb) = else_body.as_deref() {
            self.indent_here();
            self.write_str("else\n");
            self.with_indent(4, |f| {
                f.indent_here();
                f.format_node(eb);
                f.write_str(";\n");
            });
        }
        self.indent_here();
        self.write_str("fi");
    }

    pub(super) fn format_while_until(&mut self, node: &Node, keyword: &str) {
        let (NodeKind::While {
            condition,
            body,
            redirects,
            ..
        }
        | NodeKind::Until {
            condition,
            body,
            redirects,
            ..
        }) = &node.kind
        else {
            return;
        };
        self.write_str(keyword);
        self.write_char(' ');
        self.format_node(condition);
        self.write_str("; do\n");
        self.with_indent(4, |f| {
            f.indent_here();
            f.format_node(body);
            f.write_str(";\n");
        });
        self.indent_here();
        self.write_str("done");
        self.write_trailing_redirects(redirects);
    }

    pub(super) fn format_for(&mut self, node: &Node) {
        let NodeKind::For {
            var, words, body, ..
        } = &node.kind
        else {
            return;
        };
        self.format_for_or_select("for", var, words.as_deref(), body);
    }

    pub(super) fn format_select(&mut self, node: &Node) {
        let NodeKind::Select {
            var,
            words,
            body,
            redirects,
        } = &node.kind
        else {
            return;
        };
        self.format_for_or_select("select", var, words.as_deref(), body);
        self.write_trailing_redirects(redirects);
    }

    /// Shared layout for `for` and `select` loops. Takes individual
    /// fields rather than `&Node` because two `NodeKind`s share this
    /// layout.
    fn format_for_or_select(
        &mut self,
        keyword: &str,
        var: &str,
        words: Option<&[Node]>,
        body: &Node,
    ) {
        self.write_str(keyword);
        self.write_char(' ');
        self.write_str(var);
        if let Some(ws) = words {
            self.write_str(" in");
            for w in ws {
                self.write_char(' ');
                if let NodeKind::Word { value, .. } = &w.kind {
                    self.write_str(value);
                }
            }
        }
        self.write_str(";\n");
        self.indent_here();
        self.write_str("do\n");
        self.with_indent(4, |f| {
            f.indent_here();
            f.format_node(body);
            f.write_str(";\n");
        });
        self.indent_here();
        self.write_str("done");
    }

    pub(super) fn format_for_arith(&mut self, node: &Node) {
        let NodeKind::ForArith {
            init,
            cond,
            incr,
            body,
            ..
        } = &node.kind
        else {
            return;
        };
        self.write_str("for ((");
        self.write_str(init);
        self.write_str("; ");
        self.write_str(cond);
        self.write_str("; ");
        self.write_str(incr);
        self.write_str("))\n");
        self.indent_here();
        self.write_str("do\n");
        self.with_indent(4, |f| {
            f.indent_here();
            f.format_node(body);
            f.write_str(";\n");
        });
        self.indent_here();
        self.write_str("done");
    }

    pub(super) fn format_case(&mut self, node: &Node) {
        let NodeKind::Case {
            word,
            patterns,
            redirects,
            ..
        } = &node.kind
        else {
            return;
        };
        self.write_str("case ");
        if let NodeKind::Word { value, .. } = &word.kind {
            self.write_str(value);
        }
        self.write_str(" in ");
        for (i, p) in patterns.iter().enumerate() {
            if i > 0 {
                self.write_char('\n');
                self.with_indent(4, Self::indent_here);
            }
            for (j, pw) in p.patterns.iter().enumerate() {
                if j > 0 {
                    self.write_str(" | ");
                }
                if let NodeKind::Word { value, .. } = &pw.kind {
                    self.write_str(value);
                }
            }
            self.write_str(")\n");
            self.with_indent(8, |f| {
                f.indent_here();
                if let Some(body) = &p.body {
                    f.format_node(body);
                }
            });
            self.write_char('\n');
            self.with_indent(4, Self::indent_here);
            self.write_str(&p.terminator);
        }
        self.write_char('\n');
        self.indent_here();
        self.write_str("esac");
        self.write_trailing_redirects(redirects);
    }

    pub(super) fn format_function(&mut self, node: &Node) {
        let NodeKind::Function { name, body } = &node.kind else {
            return;
        };
        self.write_str("function ");
        self.write_str(name);
        self.write_str(" () \n");
        self.format_function_body(body);
    }

    fn format_function_body(&mut self, body: &Node) {
        // Bash's canonical form always wraps a function body in braces,
        // even when the source used a parens-only body like
        // `function f ( … )`.
        self.write_str("{ \n");
        let inner = if let NodeKind::BraceGroup { body: inner, .. } = &body.kind {
            inner.as_ref()
        } else {
            body
        };
        if let NodeKind::List { items } = &inner.kind {
            self.with_indent(4, |f| f.format_list_block(items));
        } else {
            self.with_indent(4, |f| {
                f.indent_here();
                f.format_node(inner);
            });
        }
        self.write_char('\n');
        self.indent_here();
        self.write_char('}');
    }

    pub(super) fn format_subshell(&mut self, node: &Node) {
        let NodeKind::Subshell {
            body, redirects, ..
        } = &node.kind
        else {
            return;
        };
        self.write_str("( ");
        self.format_node(body);
        self.write_str(" )");
        self.write_trailing_redirects(redirects);
    }

    pub(super) fn format_brace_group(&mut self, node: &Node) {
        let NodeKind::BraceGroup {
            body, redirects, ..
        } = &node.kind
        else {
            return;
        };
        self.write_str("{ ");
        self.format_node(body);
        self.write_str("; }");
        self.write_trailing_redirects(redirects);
    }

    pub(super) fn format_negation(&mut self, node: &Node) {
        let NodeKind::Negation { pipeline } = &node.kind else {
            return;
        };
        self.write_str("! ");
        self.format_node(pipeline);
    }

    pub(super) fn format_time(&mut self, node: &Node) {
        let NodeKind::Time { pipeline, posix } = &node.kind else {
            return;
        };
        if *posix {
            self.write_str("time -p ");
        } else {
            self.write_str("time ");
        }
        self.format_node(pipeline);
    }

    pub(super) fn format_coproc(&mut self, node: &Node) {
        let NodeKind::Coproc { name, command } = &node.kind else {
            return;
        };
        self.write_str("coproc ");
        if let Some(n) = name.as_deref() {
            self.write_str(n);
            self.write_char(' ');
        }
        self.format_node(command);
    }

    pub(super) fn format_conditional_expr(&mut self, node: &Node) {
        let NodeKind::ConditionalExpr { body, .. } = &node.kind else {
            return;
        };
        self.write_str("[[ ");
        self.format_cond_node(body);
        self.write_str(" ]]");
    }
}
