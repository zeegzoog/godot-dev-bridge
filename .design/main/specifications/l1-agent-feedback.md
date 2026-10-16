# Agent Feedback Channel

**Version:** 1.0.0
**Status:** Stable
**Layer:** concept

## Overview

Defines how an AI agent that drives games through godot-dev-bridge tells the project's maintainers what it found missing, broken or misleading in the bridge itself. While it works, the agent records short observations; when its work is done, it turns them into one formatted report per problem and shows the report to the human. The human either files it by hand from the text (or a prefilled link), or approves its submission to the project's public issue tracker by the agent. Nothing leaves the machine without that approval.

## Related Specifications

- [l1-driving-protocol.md](l1-driving-protocol.md) - The channel's verbs are driver-side verbs like the lifecycle verbs: they send no request, and their outcomes use the protocol's outcome classes and exit statuses (P6, P7); answer fields they add are optional (P10).
- [l1-development-only.md](l1-development-only.md) - The private directory the journal lives in (D4, D5); the channel opens no network endpoint (D6) and never involves the game or the addon.
- [l1-project-extension.md](l1-project-extension.md) - A project's adapter commands are the project's business: their absence or failure is not reported to the bridge's maintainers.
- [l2-agent-feedback-client.md](l2-agent-feedback-client.md) - Implementation of this concept.

## 1. Motivation

The bridge is made for agents, so an agent is the first to learn that a verb is missing, that an error message gave no way forward, that `--help` misled it or that a timing fact surprised it. Today that knowledge dies with the conversation: every run of the client is a short process, nothing records what went wrong, and long agent sessions are compacted, which discards what the agent noticed earlier. Maintainers hear about a gap only when a human thinks to write it up.

Two opposite failures must be avoided. Silence loses the signal. An agent that files issues on its own publishes text shaped by whatever it has read (a game's scripts, its log, a web page) under the human's identity in a public place, and many users' agents would report the same gap again and again. The channel therefore keeps observations local, makes every report reviewable as plain text, and leaves the decision to publish with the human.

## 2. Constraints & Assumptions

- The destination is the project's public issue tracker (GitHub Issues). Its address is derived from the package manifest, so the repository is named in exactly one place.
- The channel stores no credentials. When submission is offered, it uses the human's own signed-in command-line client of the tracker, the same client the installation instructions already use to download releases.
- Every run of the client is a short process. Observations must therefore survive between runs, and between game sessions of the same piece of work, as files in the user's private bridge directory.
- Reports are written in English, the language of the tracker. The conversation with the human stays in the human's language.
- GitHub facts (docs.github.com, "Creating an issue", checked 2026-10-16): a prefilled new-issue link accepts `title`, `body`, `labels`, `assignees`, `milestone`, `projects` and `template`; every parameter except title and body needs a permission the visitor may lack, and a link the visitor may not use answers 404; a link over the server's limit answers 414 and the limit is not published. The prefilled link therefore carries only title and body, and the body is cut to a conservative size budget.
- Assumption (forecast): the mechanism lives in the client as verbs plus guidance carried in its own help and answers, not as documentation alone, because only files the client keeps survive context compaction and only a verb can be gated. Runner-up: a written convention and an issue template. Override: `/magic.spec amend l1-agent-feedback.md "documentation-only convention"`.
- Assumption (forecast): the channel is on by default, with submission available behind the human's approval, because the request is for the agent itself to be able to send the report. Runner-up: drafting only by default. Override: the mode setting of the parent implementation (off, draft, full).
- Assumption (forecast): one draft becomes one issue, grouped by related notes, because separate issues can be triaged and recognized as duplicates. Runner-up: one consolidated report per piece of work. Override: `/magic.spec amend l1-agent-feedback.md "one report per session"`.

## 3. Core Invariants

Terms: a **note** is one observation the agent records; **evidence** is what the client observed itself; a **draft** is the report assembled from selected notes plus evidence; **submission** is publishing a draft to the tracker; the **human** is the person running the agent; a **notebook** is the set of notes, drafts and evidence recorded under one session name.

- **F1 Local until approved.** Notes, evidence and drafts are files in the user's private bridge directory on this machine, with the same privacy and link safety as session directories (D4, D5), and they expire after a fixed time. The channel contacts no remote service except as the direct result of a submission (F2); it has no background or deferred transmission. It never sends a request to a game and never involves the addon.
- **F2 Submission is separate, explicit and bound to a draft.** Publishing is a distinct operation from drafting. It takes the identifier of a draft the channel produced, an explicit confirmation and, optionally, the request to create despite a duplicate (F7); it takes no text, so what is published is always exactly the draft the human was shown. Once a draft has been created as an issue, repeating the submission returns that outcome instead of creating another, and two submissions of the same draft at the same time cannot both create. The confirmation records the agent's assertion that the human approved; the human's decision itself is outside the client's reach (see section 5).
- **F3 Fixed destination.** The tracker is fixed by the product. No operation accepts a repository, host, address or credential.
- **F4 Scrubbed by construction.** Every piece of text entering a note, a piece of evidence or a draft, whether agent-written or client-observed, is first scrubbed of: anything shaped like a credential; absolute filesystem paths (the project directory, the bridge directory and every other absolute path become placeholders, while `res://` and `user://` paths stay); and Markdown that would notify users or make the tracker page fetch content (mentions, issue references, raw markup, embedded images). A report never contains the game's log, screenshots, session names, or the contents of project files, scenes and resources; the one project file the channel reads is the addon's own manifest, for its version line. Scrubbing is defence in depth: the human's review of the complete text is the actual gate, and every draft states that it will be public.
- **F5 Bounded.** The number of notes and drafts in a notebook, the length of each note field, the amount of evidence and the size of a report all have fixed limits. A note or draft over a limit is rejected with a message that names the limit and the way to proceed; evidence beyond its limit replaces the oldest.
- **F6 Evidence apart from opinion.** A note states what the agent tried, what happened and, optionally, what it expected or suggests. The client adds what it observed itself: the failures of the verbs the notes name, attempts to use verbs or options that do not exist, product, addon and engine versions, platform and how the session ran. A report labels which parts the agent wrote and which the client observed. Evidence enters only through the client's own observation, never through an argument of a feedback verb.
- **F7 One problem per issue, duplicates not filed.** A draft covers one or more related notes and carries a fingerprint derived from stable facts (kind, named verbs, observed error shapes, attempted-but-missing verbs), not from free wording. Before creating an issue, submission looks for an issue that already carries the fingerprint; when one exists it reports that issue and creates nothing, unless the caller explicitly asks for a new issue. A failed look-up never blocks submission and is reported.
- **F8 A text path always exists.** In every environment (no tracker client, not signed in, offline, submission disabled) a draft yields the complete report text and, when it fits the link budget, a link that opens a prefilled new-issue page. A failed submission leaves the draft intact and retrievable.
- **F9 Honest outcomes in the protocol's classes.** Submission reports exactly one of: created (with the issue address), duplicate found (with its address), not created (with the reason and at least one recovery step), or unknown when the tracker client's answer cannot be understood (with the instruction to check the tracker before retrying). Created and duplicate are successes; not created and unknown are client failures. No error code is added (P7). It never claims creation without the tracker's confirmation.
- **F10 Guidance in band, switch out.** The client itself tells the agent, in its help text and in its answers: that the channel exists (when a game starts); when to record a note and what is not worth reporting; to write in English; to draft when the work is done (when a session stops with notes pending, its answer says so); to show the whole report to the human and offer copying it, opening the link, submitting it or discarding it; and never to submit without the human's explicit yes. The human can turn the channel off, or down to drafting only, by configuration; off records nothing, adds nothing to any answer and refuses the feedback verbs.
- **F11 Never in the way.** Recording evidence changes neither the output, the exit status nor the order of any other run, and adds no noticeable delay to it. A fault inside the channel never fails another verb.
- **F12 One notebook per session name.** Notes, drafts and evidence belong to the session name they were recorded under, so agents driving differently named sessions on one machine do not draft each other's notes. A notebook outlives the game sessions of that name: starting or pruning a session never clears it.

## 4. Detailed Design

### 4.1 Flow

```mermaid
sequenceDiagram
    participant A as Agent
    participant C as Client (private notebook)
    participant H as Human
    participant T as Issue tracker
    Note over C: every failing run is recorded as evidence
    A->>C: note (kind, summary, details, verb?)
    A->>C: stop
    C-->>A: stopped; notes pending, draft them
    A->>C: draft (note ids?)
    C-->>A: report text, link, fingerprint, draft id
    A->>H: shows the complete report, offers copy / link / submit / discard
    alt human copies or opens the link
        H->>T: pastes the text or submits the prefilled page
    else human approves submission
        A->>C: submit (draft id, confirmed)
        C->>T: duplicate look-up, then create (human's signed-in client)
        C-->>A: created / duplicate / not created / unknown
        A->>H: reports the outcome and the issue address
    end
```

### 4.2 Operations

| Operation | Effect |
| --- | --- |
| note | Adds one note to the notebook; answers with its id and the notes not yet drafted |
| list | Shows the notebook: notes (drafted or not), drafts and their outcomes |
| draft | Builds a draft from the chosen notes (default: every note not yet drafted) and the client's evidence; no network contact |
| submit | Publishes a draft to the tracker (full mode only) |
| discard | Removes chosen notes and drafts, or the whole notebook |

Note kinds: `bug`, `missing-feature`, `improvement`, `docs`. Impact: `blocked`, `workaround`, `minor`. The kind of a draft with mixed kinds is the first of that list of kinds.

### 4.3 Report Layout

```plaintext
Title: [agent-feedback] {kind}: {summary}
Body:
  ## Summary                  agent-written; one bullet per note when the draft has several
  ## What happened            agent-written: what was tried and what came back
  ## Expected / suggestion    agent-written, omitted when empty
  ## Impact                   agent-written: blocked, workaround or minor
  ## Environment              client-observed: client, addon, engine, platform, shell, session facts
  ## Client-observed context  client-observed: failures of the named verbs, attempted-but-missing verbs and options
  ## Report details           fingerprint, format version, notice that an agent drafted it and that it is public
```

### 4.4 What a Report Contains

| Reported | Never reported |
| --- | --- |
| Client version; addon version of the project; engine version; operating system and architecture; whether the shell rewrites paths; for the notebook's session: headless or windowed, whether stepping was exact, whether an adapter was configured and loaded | Project name; session name; project, home or user directory names; the adapter's path; the game's log; screenshots; scene, script or resource contents; node property values; environment variable values; the human's tracker identity |
| Per verb: failure counts and the scrubbed, shortened first line of each distinct failure; names of verbs and options that were attempted but do not exist | Request arguments as such; anything after the first line of a failure message (the client's death notes quote the game's log there) |

### 4.5 Modes

| Mode | Feedback verbs | Evidence recorded | Hints in answers | Use |
| --- | --- | --- | --- | --- |
| full (default) | note, list, draft, submit, discard | yes | yes | The agent may propose submission; the human decides |
| draft | note, list, draft, discard | yes | yes, without submission | Report text only; the agent can never publish |
| off | refused | no | no | The channel does not exist |

## 5. Drawbacks & Alternatives

- **Residual risk of the confirmation.** The confirmation cannot authenticate a human. A setup that pre-approves every command the agent runs also pre-approves submission, and an agent steered by hostile content could assert approval it never received. What bounds the damage: the text is scrubbed, bounded and visible in the conversation before and after, each draft publishes once, and the human can edit or delete the issue. Users who want a decision on every submission should not pre-approve the submit command, or should use draft mode. The cost of the gate is one extra confirmation per published report.
- **Direct submission through the tracker's web API with a stored token**: needs credential storage, an HTTP and TLS dependency and a larger attack surface in a client that has neither. Rejected; revisit only if the command-line path proves unusable for most users.
- **Recording in memory only**: impossible, every run is a separate process.
- **Recording every run as usage telemetry**: rejected; evidence is limited to failures and attempts at missing verbs, and only the evidence that concerns a note's verb reaches a report.
- **Reporting a project's missing or failing adapter commands**: rejected; they are the project's, not the bridge's, and their names would reveal the project's vocabulary.
- **Labels and assignees through the link or the client**: rejected; they need permissions the reporter may lack (a link answers 404, the client errors). Maintainers filter by the fingerprint and the title prefix.
- **Hints on every failing answer**: rejected; most failures are the agent's or the game's, and a hint on each would cost tokens and train the agent to ignore it. The hint appears when a game starts and, with notes pending, when it stops.

## Canonical References

| Alias | Path | Purpose |
| --- | --- | --- |
| `[ENTRY]` | `src/lib.rs` | Where every run's outcome is known, before it is printed |
| `[CLI]` | `src/cli.rs` | Verb definitions and help text |
| `[MANIFEST]` | `Cargo.toml` | Repository address and version |
| `[USER-CONTRACT]` | `README.md` | Where the channel is documented for users |

## Document History

| Version | Date | Author | Description |
| --- | --- | --- | --- |
| 1.0.0 | 2026-10-16 | Agent (magic.spec) | Created from the request for a mechanism that lets agents report missing features, defects and improvements of the bridge to its maintainers, as copyable text or by agent submission after the human's approval. |
