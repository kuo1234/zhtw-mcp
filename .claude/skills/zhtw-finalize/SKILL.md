---
name: zhtw-finalize
description: Run the deterministic zh-TW linter over Traditional Chinese that is about to reach a person. Use at the delivery boundary and only there: a document, README, release note, changelog entry, UI string, or a reply the user will read as the finished thing. Do not use for the Chinese an agent writes in order to work. TODO and DONE lists, plans, analyses, research and debug notes, handoff summaries, scratch files, intermediate drafts and code comments are out of scope however much Chinese they hold. Seeing Chinese is not the trigger. Finishing is.
---

# Finalizing Traditional Chinese

One command, at one moment:

```sh
zhtw-mcp lint <file> --fix --format agent
```

It applies the deterministic corrections, rescans the file it just wrote, and
prints the residue. A clean document prints `PASS` and nothing else.

Run it once, on the file being delivered. Not on every write, not on the
working documents that got you there, and not again on a file it already
passed.

## The line that decides

Ask what happens to the text next. If a person reads it as the deliverable,
finalize it. If the next reader is you, or the next turn, or a future session
picking up where this one stopped, leave it alone.

Finalize:

- A document, README, changelog, release note or specification being handed over.
- A UI string, error message or label shipping in the product.
- Chinese prose in a commit message, a pull request description or a review reply.
- A chat answer written in Chinese, when it is the answer rather than a status
  report on the way to one.

Never finalize, whatever it contains:

- Reasoning, scratch notes, and anything in a scratch directory.
- `TODO.md`, `DONE.md`, and task lists of any spelling.
- Implementation plans and design sketches still being argued with.
- Analyses, research notes, comparisons and findings written to think with.
- Debug notes, log excerpts, reproduction steps.
- Handoff summaries and context dumps for the next agent or the next session.
- Intermediate drafts, including the draft immediately before the final one.
- Working files the agent created for itself and will delete or stop reading.
- Ordinary code comments written during implementation.

The list is long because the failure it prevents is quiet. An agent that treats
Chinese as the trigger lints every one of these, several times each, and pays
for a verdict nobody will ever read. The one document that mattered gets the
same check either way.

A working document does become deliverable the moment someone asks for it as an
answer. The category is not fixed to the filename; it is fixed to what happens
next. When a plan is what the user asked to see, it is a deliverable, and it is
finalized once, at the end, not while it is being written.

## Reading the output

```text
12:8 W 軟件 -> 軟體
30:2,44:9 AMBIG 質量 ? 品質
```

`<locs> <tag> <found> -> <target>`, one line per distinct finding, every
location listed. A found span can hold a space, so split the remainder on its
last separator rather than its first.

`E`, `W` and `I` are severities, and a line carrying one has a single
determined correction. Seeing one at all after `--fix` means the fixer was not
allowed to write it: the span sat in a fenced code block, behind a suppression
pragma or in some other excluded region, or it overlapped a correction already
made. Leaving those alone is usually right, and reading the line tells you
which case it is.

`AMBIG` is the one that needs you. It means no fix tier can settle the finding
from the finding alone: several candidates, a clue-gated term whose sense has
to be read off the sentence, a term the ruleset marks as valid zh-TW in some
register, or one that online verification rejected. You are holding the
document. Pick the candidate the sentence supports and edit it by hand, or
decide the original was right and leave it.

`--explain` appends the ruleset's note to AMBIG lines and to no others. Reach
for it only when the document itself does not settle the choice, which is not
the common case.

## The bounds of the fix

`--fix` is `lexical_safe`: a term with exactly one candidate, no clue gate and
no low-confidence annotation. That tier is chosen so the command can run
unattended on something about to ship.

Do not reach for `--fix=lexical_contextual` to clear an AMBIG line. It applies
the judgment calls you were asked to make, which is the wrong trade on a
finished document. `zhtw-mcp convert` is likewise for whole-document Simplified
to Traditional conversion, not for finishing.

Add `--content-type markdown` when linting Markdown whose fenced code should
stay out of scope, `--relaxed` for UI strings, and `--profile strict` when the
project holds itself to MoE character variants. Nothing else is needed here.

## Verifying, and stopping

`PASS` on stdout is the whole verdict. Re-running a file that printed `PASS`
tells you what you already know.

If you edited AMBIG lines by hand, run the command once more. That second run
is the only rerun this workflow has, and it exists because your edit is the
thing the first run could not check.

The exit code is the gate contract, not the verdict: 0 unless a `--max-errors`
or `--max-warnings` limit was set and exceeded, or the file could not be read.
Warnings and AMBIG findings do not raise it on their own. Read stdout.

## Why this is not a save hook

`zhtw-mcp hook install` registers a PostToolUse hook that lints every Write and
Edit. It is deliberately quiet, printing nothing for a clean or unchanged file,
and it is the right answer for a repository whose Chinese is all deliverable.

It is the wrong answer for an agent that thinks in Chinese, because it cannot
tell a plan from a release note and will not try. Pick one. This skill is the
other choice: `Think, Work, Draft, Finalize, deliver`, with the linter at the
fourth step and nowhere else.

## Where the rules live

The scanner, the ruleset and the fixer are unchanged by any of this, and so is
the MCP server: `--format agent` is a rendering of the same findings the `zhtw`
tool returns. See zhtw-rules for what the linter flags and why a term is gated,
zhtw-verify for the gates a change to it has to clear, and `docs/cli.md` for
the format's exact shape.
