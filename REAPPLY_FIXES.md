# Reapply Fixes

This file tracks fixes that may need to be re-applied after upstream changes or rebases.

## 2026-02-22 - Wallet inputs losing focus on Windows
- Symptom: In the Wallet view, text inputs would activate on click and immediately lose focus, making typing impossible (observed on Windows).
- Root cause: `TextEditor` kept a pending focus target in `FocusManager` after auto-focus, so later clicks on other inputs were overridden by a repeated focus request from the earlier field.
- Fix: Request focus once and immediately clear the `FocusManager` to prevent focus stealing on subsequent frames.
- Files touched: `core/src/egui/extensions.rs`.
