// SymForge Admin UI — vanilla JS, no build step. Fetches /api/v1/* and renders
// the dashboard (economics + surface + harness), keys, and diagnostics views.
// Renders real values with clean empty / unavailable states (FR-003 / FR-006).
"use strict";

// The admin UI is served from the same origin it calls, so requests are
// same-origin and the Origin gate permits them. If a Bearer key is configured,
// the operator opens the URL the server printed (which embeds no key); fetches
// rely on the browser session being same-origin + loopback-open, or a future
// session cookie. For non-loopback keyed deploys the operator supplies the key
// via the prompt below (kept only in memory for this page session).
let API_KEY = null;

function authHeaders() {
  const h = { "Accept": "application/json" };
  if (API_KEY) h["Authorization"] = "Bearer " + API_KEY;
  return h;
}

async function api(path, opts) {
  const options = Object.assign({ headers: authHeaders() }, opts || {});
  if (options.body && !options.headers["Content-Type"]) {
    options.headers["Content-Type"] = "application/json";
  }
  const resp = await fetch("/api/v1" + path, options);
  if (resp.status === 401) {
    promptForKey();
    throw new Error("unauthorized");
  }
  return resp;
}

function promptForKey() {
  const key = window.prompt("API key required (Bearer). Paste your SymForge key:");
  if (key) API_KEY = key.trim();
}

function setStatus(msg, isError) {
  const el = document.getElementById("status-line");
  el.textContent = msg || "";
  el.className = "status-line" + (isError ? " error" : "");
}

function card(label, value, cls) {
  const div = document.createElement("div");
  div.className = "card" + (cls ? " " + cls : "");
  const l = document.createElement("div");
  l.className = "card-label";
  l.textContent = label;
  const v = document.createElement("div");
  v.className = "card-value";
  v.textContent = value;
  div.appendChild(l);
  div.appendChild(v);
  return div;
}

function note(container, text, cls) {
  container.innerHTML = "";
  const p = document.createElement("p");
  p.className = cls || "note";
  p.textContent = text;
  container.appendChild(p);
}

// --- Dashboard ---

async function loadEconomics() {
  const el = document.getElementById("economics");
  try {
    const resp = await api("/summary");
    const data = await resp.json();
    el.innerHTML = "";
    if (!data.available) {
      note(el, "Economics ledger unavailable (no durable store).", "unavailable");
      return;
    }
    if (data.total_events === 0) {
      note(el, "No economics activity recorded yet.", "empty");
      return;
    }
    el.appendChild(card("Events", String(data.total_events)));
    el.appendChild(card("Net vs manual (tokens)", String(data.total_net_vs_manual)));
    el.appendChild(card("Accepted", String(data.accepted_count)));
    el.appendChild(card("Sessions", String(data.session_count)));
  } catch (e) {
    note(el, "Failed to load economics.", "unavailable");
  }
}

async function loadSurface() {
  const el = document.getElementById("surface");
  try {
    const resp = await api("/surface");
    const data = await resp.json();
    el.innerHTML = "";
    el.appendChild(card("Profile", data.profile));
    el.appendChild(card("Tools", String(data.tool_count)));
    const list = document.createElement("div");
    list.className = "tool-list";
    list.textContent = data.tools.join(", ");
    el.appendChild(list);
  } catch (e) {
    note(el, "Failed to load surface.", "unavailable");
  }
}

async function loadHarness() {
  const el = document.getElementById("harness");
  try {
    const resp = await api("/harness");
    const data = await resp.json();
    el.innerHTML = "";
    if (!data.available) {
      note(el, "Harness registry unavailable.", "unavailable");
      return;
    }
    if (!data.entries.length) {
      note(el, "No known harness clients detected.", "empty");
      return;
    }
    const table = document.createElement("table");
    table.className = "harness-table";
    table.innerHTML = "<thead><tr><th>Client</th><th>State</th><th>Config</th></tr></thead>";
    const tbody = document.createElement("tbody");
    data.entries.forEach(function (entry) {
      const tr = document.createElement("tr");
      tr.appendChild(td(entry.name));
      tr.appendChild(td(entry.state.replace(/_/g, " ")));
      tr.appendChild(td(entry.config_path, "path-cell"));
      tbody.appendChild(tr);
    });
    table.appendChild(tbody);
    // Wrap in a horizontal scroll container so long config paths scroll within
    // the card instead of widening the page on narrow viewports (006 mobile
    // overflow fix); desktop is unaffected (the table still fills its width).
    const scroll = document.createElement("div");
    scroll.className = "table-scroll";
    scroll.appendChild(table);
    el.appendChild(scroll);
  } catch (e) {
    note(el, "Failed to load harness status.", "unavailable");
  }
}

function td(text, cls) {
  const cell = document.createElement("td");
  cell.textContent = text;
  if (cls) cell.className = cls;
  return cell;
}

// --- AAP integration panel (008 US1 + US2) ---

// A labelled, copy-to-clipboard code block for an integration preset snippet.
function snippetBlock(label, code) {
  const wrap = document.createElement("div");
  wrap.className = "snippet";
  const head = document.createElement("div");
  head.className = "snippet-head";
  const l = document.createElement("span");
  l.className = "snippet-label";
  l.textContent = label;
  const copy = document.createElement("button");
  copy.type = "button";
  copy.className = "action snippet-copy";
  copy.textContent = "Copy";
  copy.addEventListener("click", function () {
    const done = function () { copy.textContent = "Copied"; setTimeout(function () { copy.textContent = "Copy"; }, 1500); };
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(code).then(done, function () { setStatus("Copy failed; select manually.", true); });
    } else {
      // Fallback: select the text so the operator can copy manually.
      done();
    }
  });
  head.appendChild(l);
  head.appendChild(copy);
  const pre = document.createElement("pre");
  pre.className = "snippet-code";
  pre.textContent = code;
  wrap.appendChild(head);
  wrap.appendChild(pre);
  return wrap;
}

async function loadAap() {
  const el = document.getElementById("aap");
  try {
    const resp = await api("/aap");
    const data = await resp.json();
    el.innerHTML = "";
    if (!data.detected) {
      // Clean empty state — not an error (SC-002). The rest of the dashboard is
      // unaffected.
      note(el, "AAP not detected (no sibling checkout or AAP_ROOT).", "empty");
      return;
    }
    // Detection + mode + drift cards.
    el.appendChild(card("Detected", "yes (" + (data.source || "?") + ")"));
    el.appendChild(card("Root", data.root || "(unknown)", "path-cell"));
    el.appendChild(card("Mode", data.mode));
    el.appendChild(card("Pinned version", data.pinned_version || "(pin unknown)"));
    el.appendChild(card("Running version", data.running_version));
    // Drift warning is raised iff the pin has actually drifted (no false positive
    // for pin_unknown).
    if (data.drifted) {
      el.appendChild(card(
        "Drift",
        "WARNING: AAP pins " + data.pinned_version + " but the running crate is " + data.running_version + " — re-pin AAP's embed dependency.",
        "drift-warning"
      ));
    } else if (data.drift === "match") {
      el.appendChild(card("Drift", "none (pin matches running)", "drift-ok"));
    } else {
      el.appendChild(card("Drift", "pin unknown (no false warning)", "drift-unknown"));
    }
    if (data.indexed_roots && data.indexed_roots.length) {
      el.appendChild(card("Indexed roots", data.indexed_roots.join(", "), "path-cell"));
    }
    // Presets (US2): the embed snippet is always present for a detected AAP; the
    // serve-URL preset only when serve is active.
    if (data.presets) {
      if (data.presets.embed_snippet) {
        el.appendChild(snippetBlock("Embed (Cargo.toml)", data.presets.embed_snippet));
      }
      if (data.presets.serve_url_snippet) {
        el.appendChild(snippetBlock("Serve URL (MCP registration)", data.presets.serve_url_snippet));
      }
    }
  } catch (e) {
    note(el, "Failed to load AAP status.", "unavailable");
  }
}

// --- Keys ---

async function loadKeys() {
  const body = document.getElementById("keys-body");
  try {
    const resp = await api("/keys");
    const data = await resp.json();
    body.innerHTML = "";
    if (!data.available) {
      body.innerHTML = '<tr><td colspan="5" class="unavailable">Key store unavailable (bootstrap --api-key still works).</td></tr>';
      return;
    }
    if (!data.keys.length) {
      body.innerHTML = '<tr><td colspan="5" class="empty">No API keys yet. Mint one above.</td></tr>';
      return;
    }
    data.keys.forEach(function (k) {
      const tr = document.createElement("tr");
      tr.appendChild(td(k.label));
      tr.appendChild(td(k.fingerprint));
      tr.appendChild(td(new Date(k.created_ms).toLocaleString()));
      tr.appendChild(td(k.active ? "active" : "revoked"));
      const actions = document.createElement("td");
      if (k.active) {
        actions.appendChild(actionButton("Rotate", function () { rotateKey(k.id); }));
        actions.appendChild(actionButton("Revoke", function () { revokeKey(k.id); }));
      }
      tr.appendChild(actions);
      body.appendChild(tr);
    });
  } catch (e) {
    body.innerHTML = '<tr><td colspan="5" class="unavailable">Failed to load keys.</td></tr>';
  }
}

function actionButton(label, onClick) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "action";
  b.textContent = label;
  b.addEventListener("click", onClick);
  return b;
}

function showRawSecret(raw) {
  const el = document.getElementById("mint-result");
  el.className = "mint-result";
  el.innerHTML = "";
  const warn = document.createElement("p");
  warn.className = "secret-warn";
  warn.textContent = "Copy this secret now — it is shown only once:";
  const code = document.createElement("code");
  code.className = "secret";
  code.textContent = raw;
  el.appendChild(warn);
  el.appendChild(code);
}

async function mintKey(label) {
  try {
    const resp = await api("/keys", { method: "POST", body: JSON.stringify({ label: label }) });
    const data = await resp.json();
    if (data.raw_secret) showRawSecret(data.raw_secret);
    await loadKeys();
  } catch (e) {
    setStatus("Failed to mint key.", true);
  }
}

async function rotateKey(id) {
  try {
    const resp = await api("/keys/" + id + "/rotate", { method: "POST" });
    const data = await resp.json();
    if (data.raw_secret) showRawSecret(data.raw_secret);
    await loadKeys();
  } catch (e) {
    setStatus("Failed to rotate key.", true);
  }
}

async function revokeKey(id) {
  try {
    await api("/keys/" + id, { method: "DELETE" });
    await loadKeys();
  } catch (e) {
    setStatus("Failed to revoke key.", true);
  }
}

// --- Diagnostics ---

async function loadSystem() {
  const el = document.getElementById("system");
  try {
    const resp = await api("/system");
    const data = await resp.json();
    el.innerHTML = "";
    el.appendChild(card("PID", String(data.pid)));
    el.appendChild(card("Uptime (s)", String(data.uptime_secs)));
    el.appendChild(card("Active sessions", String(data.active_sessions)));
    el.appendChild(card("Indexed files", String(data.indexed_file_count)));
    el.appendChild(card("Indexed symbols", String(data.indexed_symbol_count)));
    el.appendChild(card("Index generation", String(data.index_generation)));
    const projects = data.indexed_projects.length ? data.indexed_projects.join(", ") : "(none)";
    el.appendChild(card("Indexed projects", projects));
    document.getElementById("project-name").textContent =
      data.indexed_projects[0] || "(no project)";
  } catch (e) {
    note(el, "Failed to load system telemetry.", "unavailable");
  }
}

// --- View switching + refresh ---

function switchView(name) {
  document.querySelectorAll(".tab").forEach(function (t) {
    t.classList.toggle("active", t.dataset.view === name);
  });
  document.querySelectorAll(".view").forEach(function (v) {
    v.classList.toggle("active", v.id === "view-" + name);
  });
}

async function refreshAll() {
  setStatus("Refreshing…");
  await Promise.all([
    loadEconomics(),
    loadSurface(),
    loadHarness(),
    loadAap(),
    loadKeys(),
    loadSystem(),
  ]);
  setStatus("Updated " + new Date().toLocaleTimeString());
}

document.addEventListener("DOMContentLoaded", function () {
  document.querySelectorAll(".tab").forEach(function (t) {
    t.addEventListener("click", function () { switchView(t.dataset.view); });
  });
  document.getElementById("refresh").addEventListener("click", refreshAll);
  document.getElementById("mint-form").addEventListener("submit", function (e) {
    e.preventDefault();
    const label = document.getElementById("mint-label").value || "api key";
    mintKey(label);
  });
  refreshAll();
});
