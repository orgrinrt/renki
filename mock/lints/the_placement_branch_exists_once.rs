//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! How a backend's material is placed is decided in one body, `place` in `extension.rs`.
//!
//! `materialise_once` names the backend at compile time and `locate` finds it in a registry, and
//! both go through `place`, so a precondition added there reaches both. The defect this holds out
//! is the other shape: the branch on `places_itself` open-coded a second time, in `locate` or
//! anywhere else, so that two dispatches are separate bodies that merely agree until one is
//! edited. It was found once, with the two already drifted apart, and no runtime test can see it,
//! because both bodies behave the same until the day they do not.
//!
//! The lint reads the parse of `src/extension.rs` in the `renki` crate and finds every `if` and
//! `match` whose condition names `places_itself`, as a call or as the argument `place` takes. There
//! must be exactly one, and it must sit inside `fn place`. A call that merely hands the answer on,
//! `place(root, B::places_itself(), ..)`, is not a branch. A comment or a doc line that quotes one
//! is not code and is not read, which the text-reading test this replaces could not tell.

use mockspace::{CrateLint, Lint, LintContext, LintError, Severity};

pub fn lint() -> Box<dyn CrateLint> {
    Box::new(ThePlacementBranchExistsOnce)
}

const NAME: &str = "the-placement-branch-exists-once";

/// The crate the property is about.
const CRATE: &str = "renki";

/// The file, relative to the crate, that holds the one body.
const FILE: &str = "src/extension.rs";

/// What a branch on the placement names.
const DECIDER: &str = "places_itself";

/// The one function the branch may sit in.
const PLACE: &str = "place";

struct ThePlacementBranchExistsOnce;

impl Lint for ThePlacementBranchExistsOnce {
    fn name(&self) -> &'static str {
        NAME
    }

    fn description(&self) -> &'static str {
        "the branch on a backend's `places_itself` exists once in `extension.rs`, inside `place`"
    }

    fn default_severity(&self) -> Severity {
        Severity::HARD_ERROR
    }

    fn per_file(&self) -> bool {
        false
    }

    fn markers_decide(&self) -> bool {
        false
    }
}

/// One branch on the placement: the line it opens on and the function it sits in, empty when it
/// sits in none.
#[derive(Debug, PartialEq, Eq)]
pub struct Branch {
    pub line:   usize,
    pub inside: String,
}

/// Every `if` and `match` in `text` whose condition names the placement, in order.
pub fn branches(text: &str) -> Vec<Branch> {
    let mut out = Vec::new();
    let Some(tree) = mockspace::make_parser().parse(text, None) else {
        return out;
    };
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let condition = match node.kind() {
            "if_expression" => node.child_by_field_name("condition"),
            "match_expression" => node.child_by_field_name("value"),
            _ => None,
        };
        let names_it = condition
            .and_then(|c| c.utf8_text(text.as_bytes()).ok())
            .is_some_and(|c| c.contains(DECIDER));
        if names_it {
            let mut inside = String::new();
            let mut up = node.parent();
            while let Some(parent) = up {
                if parent.kind() == "function_item" {
                    inside = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(text.as_bytes()).ok())
                        .unwrap_or_default()
                        .to_string();
                    break;
                }
                up = parent.parent();
            }
            out.push(Branch {
                line: node.start_position().row + 1,
                inside,
            });
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return out;
            }
        }
    }
}

/// What is wrong with the placement branches of one file, as messages with the line each belongs
/// to. Empty when there is exactly one and it sits in `place`.
pub fn findings(text: &str) -> Vec<(usize, String)> {
    let found = branches(text);
    let mut out = Vec::new();
    match found.as_slice() {
        [] => out.push((
            1,
            format!(
                "no body in `{FILE}` branches on `{DECIDER}`, so `{PLACE}` is gone or has moved and \
                 this lint is measuring nothing"
            ),
        )),
        [only] => {
            if only.inside != PLACE {
                let place = if only.inside.is_empty() {
                    "outside any function".to_string()
                } else {
                    format!("in `{}`", only.inside)
                };
                out.push((
                    only.line,
                    format!(
                        "the placement branch sits {place}, and the one body that decides is \
                         `{PLACE}`: everything else hands the answer to it"
                    ),
                ));
            }
        },
        many => {
            let lines: Vec<String> = many.iter().map(|b| b.line.to_string()).collect();
            for extra in many.iter().filter(|b| b.inside != PLACE).chain(
                many.iter()
                    .filter(|b| b.inside == PLACE)
                    .skip(1),
            ) {
                out.push((
                    extra.line,
                    format!(
                        "the placement branch exists more than once (lines {}), so a precondition \
                         added to one does not reach the other; the one body is `{PLACE}`, and \
                         this should hand the answer to it",
                        lines.join(", ")
                    ),
                ));
            }
        },
    }
    out
}

impl CrateLint for ThePlacementBranchExistsOnce {
    fn check(&self, ctx: &LintContext) -> Vec<LintError> {
        if ctx.crate_name != CRATE {
            return Vec::new();
        }
        let Some(file) = ctx
            .all_sources
            .iter()
            .find(|f| f.rel_path.to_string_lossy().replace('\\', "/") == FILE)
        else {
            return vec![{
                let mut e = LintError::error(
                    ctx.crate_name.to_owned(),
                    1,
                    NAME,
                    format!(
                        "`{FILE}` is not in the crate, so the placement branch is measured by nothing"
                    ),
                );
                e.path = Some(FILE.to_string());
                e
            }];
        };
        findings(&file.text)
            .into_iter()
            .map(|(line, message)| {
                let mut e = LintError::error(ctx.crate_name.to_owned(), line, NAME, message);
                e.path = Some(FILE.to_string());
                e
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use mockspace::testkit::LintFixture;

    use super::*;

    /// The shape the real file has: the one body, and the two routes in that hand it the answer.
    const ONE_BODY: &str = "\
/// An `if places_itself {` quoted in a doc line is not a branch.
pub fn materialise_once<B: Backend>(plan: &B::Plan, root: &Path) -> Result<(), String> {
    place(root, B::places_itself(), |into| B::materialise(plan, into))
}

pub fn locate(backend: &Registered, root: &Path) -> Result<(), String> {
    // if backend.places_itself() { ... } is what this must not become
    place(root, backend.places_itself(), |into| backend.materialise(into))
}

pub(crate) fn place(root: &Path, places_itself: bool, run: impl FnOnce(&Path)) {
    if places_itself {
        return run(root);
    }
    scratch(root, run)
}
";

    fn ctx_for(crate_name: &str, extension: &str) -> Vec<LintError> {
        let fixture = LintFixture::new("")
            .with_crate_name(crate_name, crate_name)
            .with_module("src/extension.rs", extension);
        ThePlacementBranchExistsOnce.check(&fixture.ctx())
    }

    #[test]
    fn the_one_body_in_place_passes() {
        assert!(ctx_for("renki", ONE_BODY).is_empty());
        assert_eq!(branches(ONE_BODY), vec![Branch {
            line:   12,
            inside: "place".to_string(),
        }]);
    }

    #[test]
    fn a_second_copy_in_locate_fails() {
        let twice = ONE_BODY.replace(
            "    place(root, backend.places_itself(), |into| backend.materialise(into))",
            "    if backend.places_itself() {\n        return backend.materialise(root);\n    }\n    place(root, false, |into| backend.materialise(into))",
        );
        let errors = ctx_for("renki", &twice);
        assert_eq!(
            errors.len(),
            1,
            "{:?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
        assert!(
            errors[0].message.contains("more than once"),
            "{}",
            errors[0].message
        );
        assert_eq!(errors[0].line, 8);
    }

    #[test]
    fn a_match_on_the_answer_is_a_branch_too() {
        let twice = ONE_BODY.replace(
            "    place(root, backend.places_itself(), |into| backend.materialise(into))",
            "    match backend.places_itself() {\n        true => backend.materialise(root),\n        false => place(root, false, |into| backend.materialise(into)),\n    }",
        );
        assert_eq!(ctx_for("renki", &twice).len(), 1);
    }

    #[test]
    fn the_only_branch_outside_place_fails() {
        let moved = "\
pub fn locate(backend: &Registered, root: &Path) -> Result<(), String> {
    if backend.places_itself() {
        return backend.materialise(root);
    }
    scratch(root)
}
pub(crate) fn place(root: &Path) {}
";
        let errors = ctx_for("renki", moved);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("in `locate`"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn no_branch_at_all_fails_rather_than_passing_silently() {
        let errors = ctx_for("renki", "pub(crate) fn place(root: &Path) {}\n");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("measuring nothing"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn a_missing_file_fails_in_the_crate_and_is_nothing_in_another() {
        let fixture = LintFixture::new("").with_crate_name("renki", "renki");
        let errors = ThePlacementBranchExistsOnce.check(&fixture.ctx());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("is not in the crate"),
            "{}",
            errors[0].message
        );

        let other = LintFixture::new("").with_crate_name("renki-dirs", "dirs");
        assert!(ThePlacementBranchExistsOnce.check(&other.ctx()).is_empty());
        assert!(ctx_for("renki-config", "pub fn x() {}").is_empty());
    }

    #[test]
    fn a_branch_in_a_test_module_inside_the_file_is_still_a_second_body() {
        let with_tests = format!(
            "{ONE_BODY}\n#[cfg(test)]\nmod tests {{\n    fn pick() {{\n        if super::B::places_itself() {{}}\n    }}\n}}\n"
        );
        assert_eq!(ctx_for("renki", &with_tests).len(), 1);
    }
}
