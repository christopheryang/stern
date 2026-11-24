# Model-Backed Chat

- Date: 2026-11-22
- Status: In progress (slice 1 landed on `feat/model-backed-chat`)
- Related: [2026-11-21 - AI-and-Models](2026-11-21%20-%20AI-and-Models.md), ADR-0008 (runtime download)

## Why

The downloaded model was never run: chat classification was entirely
rule-based, and `AiModel::run_inference` was a stub written for a
BERT-style classifier (a hand-built ~100-word tokenizer, outputs
discarded), while every catalog model is a generative chat LM
(SmolLM / SmolLM2 / Qwen instruct models). This feature makes the
downloaded model part of the chat — under a strict product rule:

**No prose.** The user never reads model-generated text. Conversations
are simple and structured: short questions, lettered choices, and
direct answers. The model only ever *parses* — it turns a free-form
message into a structured request that the deterministic code then
executes. Everything the user sees is a template.

## The guided conversation (slice 1, landed)

Storing a secret becomes a short interview when a request does not
carry everything needed:

1. "What would you like to save?" — a. note, b. document,
   c. username/password, d. credit card, e. contact info. A request
   that names the kind ("save a note") skips this question.
2. One question per field, in order (for a login: "What website is
   this username/password for?", then username, then password).
   Optional fields (a contact's email, address) can be skipped.
3. A confirmation summarizing the entry — sensitive values (password,
   CVV, card number, text content) masked — with yes/no options.

Answers to a choice can be the letter or the label. "cancel" (also
"stop", "quit", "never mind") aborts at any stage; nothing is stored
until the confirmation says yes.

Kinds and their fields (the first field is the entry's name):

| Kind | Fields (in ask order) |
| --- | --- |
| note (`text`) | title, text |
| document (`document`) | title, content |
| username/password (`login`) | website, username, password |
| credit card (`card`) | label, number, expiry, cvv |
| contact info (`contact`) | name, phone, email (optional), address (optional) |

Implementation: `core/stern-core/src/ai/conversation.rs` is a
deterministic state machine producing the existing `Intent` values;
`ChatHandler` owns the in-progress conversation and routes messages
to it. `ChatResponse` (core and IPC DTO) gained `options: Vec<String>`
carrying the lettered choices. Triggers:

- A store request with no content at all ("store") starts the full
  interview, kind question first.
- A parse with a value but no name is prefilled into the interview,
  which asks for the missing name and fields.
- A store the parser named executes immediately, exactly as before —
  one-shot commands keep their one-shot behavior.

## Retrieval questions (slice 2, landed)

"What's my password for Google?" resolves to an entry and a field and
answers with the value. One match answers directly; several matches
ask a lettered disambiguation question ("which one? a. Google (work),
b. Google (personal)"); no match says so plainly.

As landed: `Intent::RetrieveSecret` gained a `field` (extracted from
the question's field words), the chat command executes the search
against the real vault (the standalone entry commands are still
stubs; the chat path lists entries, matches on the plaintext entry
name, and decrypts only the answering entry), and `ChatHandler` holds
the pending lettered choice, answering a pick with a reveal action.
A sentence-initial "what's"/"where's" is expanded before
classification — the detectors key on the separate words — which is
what makes the "what's my password for ...?" phrasing parse at all.

## The model as parser (slice 3, landed)

- Each catalog model downloads its `tokenizer.json` (hash-pinned,
  like the model file) and loads it with the `tokenizers` crate. A
  model installed before this change has no tokenizer on disk; the
  chat layer then behaves as if no model were loaded (rules only)
  until the model is removed and downloaded again.
- `AiModel` has a real generation loop: KV-cache decoding, greedy,
  96-token cap, layer/head geometry read from the model's own input
  metadata. Sessions are pinned to the CPU provider with an explicit
  `PreferCPU` device policy — ort's builder otherwise defaults to
  `MaxEfficiency`, which hands plain graphs to CoreML on macOS, and
  CoreML rejects the zero-length KV cache a decoder starts with.
- The model parses free-form messages into the same structured
  requests the rule classifier and the guided conversation use —
  store parses feed `Conversation::prefilled`, retrieval parses feed
  slice 2's resolver. Output is strict JSON, validated before use;
  any failure falls back to the rule classifier. The model's text is
  never shown to the user.
- Prompts carry the same six worked examples in both formats —
  ChatML turns for the instruct models, `Message:` / `Command:`
  completion for the base SmolLM-135M. An abstract spec prompt was
  measured first and failed: the small models rambled, echoed the
  spec as values, or answered refusal prose. Examples work.
- Measured parse quality (ignored smoke test
  `core/stern-core/tests/model_parse_smoke.rs`, 10 messages): the
  default SmolLM-135M parses 8 of 10 itself and defers 2 to the
  rules; SmolLM2-135M-Instruct 6, with 2 deferrals and 2 "greeting"
  misfires that a guard in `ChatHandler` catches by cross-checking
  greetings against the rule classifier; Qwen2.5-0.5B-Instruct 7
  with 3 deferrals and no misfires, at ~3 s per parse against ~1 s
  for the 135M models. Every message ends up handled correctly
  across the model + guard + rules combination. (These figures were
  measured on the catalog as it stood then; SmolLM-135M has since been
  retired and the tiers are Small = SmolLM2-135M-Instruct and
  Large = Qwen2.5-0.5B-Instruct — ADR-0010, ADR-0014; a Medium tier,
  SmolLM2-360M-Instruct, existed briefly.)

## UI (slice 4, landed)

The chat view renders `options` as tappable choices under the newest
assistant message; tapping one sends its label as the next message,
which the guided conversation, the confirmation prompt, and the
disambiguation flow all accept — so none of them ever requires
typing a letter.

## First-run fixes (landed Nov 23)

Three walkthrough findings, fixed together:

- **Small talk.** "How are you?" fell through to the "I'm not sure
  what you mean" reply: the greeting/help checks compared whole
  words, and the sentence's last word carries its punctuation
  ("you?"), and no small-talk phrases were recognized at all. The
  checks now strip trailing punctuation and recognize phrases
  ("how are you", "how's it going", "what's up") and short
  gratitude messages. A greeting gets a short, normal answer first,
  then a one-line summary of what Stern can do.
- **Secret typing.** Responses whose reply is a secret (a
  sensitive guided-conversation field, a master password) carry
  `input_sensitive`. The chat input then runs in peek-mask mode:
  each typed character stays visible for two seconds, then turns
  into `*`. The mask is character-for-character; edits are folded
  back into the real value by diffing the display against its
  last rendered form. A sent secret appears in the transcript
  only as a mask.
- **The vault gate.** Nothing could be stored before a vault
  existed, yet nothing created one: a first store collected the
  whole secret and then failed with "Vault is locked." Following
  the pattern of KeePass and Bitwarden — the vault is set up
  first, before anything else can happen — the chat handler now
  knows the vault's state (synced by the app layer per message)
  and runs a gate conversation before any vault-touching
  request: for a new vault, an introduction, a master password
  (minimum 8 characters, passphrase suggested, no-recovery
  warning), and a confirmation typing; for a locked vault, the
  master password, re-prompted on failure under the existing
  unlock lockout. When the gate completes, the interrupted
  message is replayed, so the original request continues where
  it left off. Small talk and help are never gated.

## The front door (landed Nov 23)

The Tauri vault commands (`create_vault`, `unlock_vault`,
`vault_status`, `lock_vault`) existed since the initial
implementation, but no UI ever called them — the app opened
straight into chat. The shell now gates on the vault before
anything else:

- **First run — setup screen.** Choose a master password (typed
  twice; policy: 8+ characters with a lowercase letter, an
  uppercase letter, a number, and a symbol (a space is not one) — enforced here and
  in the chat gate). The screen itself is deliberately brief —
  labeled fields and one requirements line, the pattern
  Bitwarden and other open-source vaults use — and the
  no-recovery warning is its own step, shown after a valid
  password is entered, before anything is created. There is no
  account and no username: the password plus the device's
  Secret Key (2SKD) is the whole credential, the pattern local
  products (`KeePass`, `Enpass`) use — usernames exist only
  where a server account does.
- **Every later launch — login screen.** The vault starts
  locked; the master password opens it (the existing failed-
  attempt lockout applies). The padlock in the top bar returns
  to this screen.
- **Order.** The vault gate is outermost: Settings — and with
  it any model download — is unreachable until the vault is
  unlocked, so setup precedes the model gate, which precedes
  the chat. The chat-level setup/unlock conversation remains
  as the backstop for a vault that locks mid-session.
