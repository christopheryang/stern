// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT
//
// A stand-in for the Tauri bridge, used only to take the screenshots in
// the User Guide (see "Regenerating the guide's screenshots" in
// docs/Development.md). It is injected into a copy of the built
// frontend, never into the app. The chat replies are the real ones the
// Rust chat handler produces (core/stern-core/src/ai).
//
// Page query parameters pick the starting state:
//   ?phase=setup|login|unlocked   which vault screen to start on
//   ?model=none|downloading|ready AI model state (default: ready)
//   ?play=<steps>                 messages to send once the chat loads,
//                                 separated by "|" (see below)
//   ?page=settings|help           open that page after load
//   ?theme=light|dark             set the theme (default: light)
//   ?fill=<password>              type it into both setup fields
//
// A one-shot headless screenshot cannot click, so `play` drives the
// real UI: each step types into the chat box and presses Enter. A step
// "@yes" taps the option with that label instead. A step that ends in
// "~" is typed but not sent, which shows the masked input mid-peek:
// all but its last five letters have already turned into the mask.

(() => {
  const q = new URLSearchParams(location.search);
  const phase = q.get("phase") || "unlocked";
  const modelMode = q.get("model") || "ready";
  const theme = q.get("theme") || "light";
  // Entrance animations would be caught half-way by a screenshot.
  const still = document.createElement("style");
  still.textContent = "*,*::before,*::after{animation:none!important;transition:none!important}";
  document.documentElement.appendChild(still);
  try { localStorage.setItem("stern-theme", theme); } catch (e) {}

  const small = {
    model_id: "s", display_name: "Small",
    description: "Quick and light. Best for short, simple requests.",
    pros: ["Replies quickly", "Small download", "Light on your computer's memory"],
    cons: ["May misread longer or unusual wording"],
    recommended: true, state: "not_installed", installed_version: null,
    latest_version: "1", size_bytes: 118000000, is_active: false,
    runtime_installed: false, is_downloading: false, downloaded_bytes: 0, total_bytes: 0,
  };
  const large = {
    model_id: "l", display_name: "Large",
    description: "Understands you best, but is slower.",
    pros: ["Understands more ways of saying things", "Fewer misunderstandings"],
    cons: ["Takes a few seconds to reply", "Several times bigger to download",
      "Uses more of your computer's memory"],
    recommended: false, state: "not_installed", installed_version: null,
    latest_version: "1", size_bytes: 483000000, is_active: false,
    runtime_installed: false, is_downloading: false, downloaded_bytes: 0, total_bytes: 0,
  };
  const models = () => {
    if (modelMode === "ready") {
      return [
        { ...small, state: "installed", installed_version: "1", is_active: true, runtime_installed: true },
        { ...large, runtime_installed: true },
      ];
    }
    if (modelMode === "downloading") {
      return [{ ...small, is_downloading: true, downloaded_bytes: 61000000, total_bytes: 118000000 }, large];
    }
    return [small, large];
  };

  const reply = (message, extra = {}) => ({
    message, action: null, user_message_display: null,
    options: [], input_sensitive: false, ...extra,
  });

  // A tiny script of the real guided conversations.
  let stage = "idle";
  let title = "";
  const chat = (text) => {
    const t = text.trim().toLowerCase();
    if (stage === "title") {
      title = text.trim(); stage = "text";
      return reply("What is your note?", { input_sensitive: true });
    }
    if (stage === "text") {
      stage = "confirm";
      return reply(`Save this note?\n  title: ${title}\n  text: ••••`, { options: ["yes", "no"] });
    }
    if (stage === "confirm") {
      stage = "idle";
      if (t === "yes") {
        return reply(`Saved your note "${title}".\n  title: ${title}\n  text: ••••`, { action: "store" });
      }
      return reply("Okay, I won't save it.");
    }
    if (t === "save a note" || t === "store a note") {
      stage = "title"; return reply("What should I call this note?");
    }
    if (t === "store" || t === "save something") {
      return reply("What would you like to save?",
        { options: ["note", "document", "username/password", "credit card", "contact info"] });
    }
    if (t.startsWith("find my")) {
      return reply(`"Shopping list":\n  title: Shopping list\n  text: eggs, milk and bread`);
    }
    if (t === "help") {
      return reply("I can help you keep your notes and documents safe. Here's what I can do:\n\n" +
        "🔐 Save a note -- \"save a note\"\n🔍 Find something -- \"find my shopping list\"\n" +
        "📄 Save a document -- \"save a document\"\n🔑 Save a password -- \"save my GitHub password\"\n" +
        "Tip: for notes you want to keep forever, a plain text file (.txt) is best. " +
        "Any computer can open it, with no special app.");
    }
    return reply(`I'm not sure what you mean by "${text}".\n\nTry something like:\n` +
      "- \"save a note\"\n- \"find my shopping list\"\n- \"save a document\"\n- \"help\"");
  };

  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        if (cmd === "vault_status") {
          return { is_initialized: phase !== "setup", is_unlocked: phase === "unlocked", entry_count: 1 };
        }
        if (cmd === "model_status") return models();
        if (cmd === "send_chat_message") {
          // The UI serializes its arguments with serde-wasm-bindgen,
          // which can hand over Maps instead of plain objects.
          const plain = (v) => (v instanceof Map ? Object.fromEntries([...v].map(([k, x]) => [k, plain(x)])) : v);
          return chat(plain(args).request.content);
        }
        return {};
      },
    },
  };

  // ── Auto-play, for screenshots only ───────────────────────────────
  const play = (q.get("play") || "").split("|").filter(Boolean);
  const pageName = q.get("page");
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const until = async (find, ms = 4000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) { const el = find(); if (el) return el; await wait(50); }
    return null;
  };
  const writeValue = (el, value) => {
    const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, value);
  };
  const setValue = (el, value) => {
    writeValue(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const box = () => document.querySelector(".composer input, .composer textarea");
  // One keystroke: add a character to what the input shows (for the
  // masked input that is the mask, as with a real keyboard) and put the
  // caret after it, which is how the app locates the edit.
  const typeChar = async (el, ch) => {
    writeValue(el, el.value + ch);
    el.setSelectionRange(el.value.length, el.value.length);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    await wait(25);
  };
  const typeInto = async (text) => {
    const el = await until(box);
    el.focus();
    for (const ch of text) { await typeChar(el, ch); }
    return el;
  };
  window.addEventListener("load", async () => {
    await wait(700);
    const fill = q.get("fill");
    if (fill) {
      for (const input of document.querySelectorAll("form input[type=password], form input[type=text]")) {
        if (/password|same/i.test(input.placeholder)) { input.focus(); setValue(input, fill); }
      }
    }
    if (pageName) {
      const label = pageName === "settings" ? "Settings" : "Help";
      (await until(() => document.querySelector(`button[aria-label="${label}"]`)))?.click();
    }
    for (const step of play) {
      if (step.startsWith("@")) {
        const label = step.slice(1);
        const btn = await until(() => [...document.querySelectorAll("button.option")].find((b) => b.textContent.includes(label)));
        btn?.click();
      } else if (step.endsWith("~")) {
        // Type most of it, let those letters expire into the mask, then
        // type the last few, so the shot shows both states of the peek.
        const text = step.slice(0, -1);
        const head = text.slice(0, -5);
        const el = await typeInto(head);
        await wait(2300);
        for (const ch of text.slice(-5)) { await typeChar(el, ch); }
      } else {
        const el = await typeInto(step);
        el.closest("form")?.requestSubmit();
      }
      await wait(500);
      // The app scrolls to the newest message on a timer that a
      // one-shot headless run does not wait for, so do it here.
      const t = document.querySelector(".transcript");
      if (t) t.scrollTop = t.scrollHeight;
    }
    await wait(300);
    document.title = "ready";
  });
})();
