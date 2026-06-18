# Contract: testability seams (FR-017/018)

Mirrors the shipped `OnboardingSink` seam. Every operator-facing side effect is injectable
so the whole wizard/admin flow runs in tests with scripted answers and zero real terminal/
network/browser/process effects.

## `SetupSink`
```text
trait SetupSink {
    fn status(&mut self, line: &str);              // progress / summary lines
    fn ask_choice(&mut self, q: &str, opts: &[&str]) -> usize;  // pick an option
    fn confirm(&mut self, action_plan: &str) -> bool;           // restate + confirm (FR-008)
}
```
- `StderrSetupSink` (real): prints to stderr, reads stdin.
- `ScriptedSetupSink` (test): pre-supplied answers, records prompts; no terminal read (FR-014).

## `BrowserOpener`
```text
trait BrowserOpener { fn open_url(&self, url: &str) -> OpenOutcome; }  // Opened | Skipped
```
- Real: `std::process::Command` OS opener (`start`/`open`/`xdg-open`); headless/no-opener →
  `Skipped` (print only, never error) (FR-011).
- `NoopBrowserOpener` (test): records the URL, returns `Skipped`.

## Port probe + harness apply
- Port selection is exercised through the probe helper (free-port.md); tests assert
  selection without depending on a fixed real port.
- Harness apply/backup is already fixtures-driven (`HarnessRegistry::known_with(home,
  working_dir)` over a temp dir + `harness_apply`) — no real operator config is touched (FR-018).

## Invariant (SC-006)
The complete flow validates with fixtures only; no test mutates a real harness config,
reads the real terminal, contacts the network beyond a localhost bind, or opens a browser.
