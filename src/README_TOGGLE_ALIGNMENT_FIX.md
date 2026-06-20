# Settings toggle alignment fix

Cause:
- The global `button` rule applies `min-height: 34px`.
- `.settings-toggle` only set `height: 23px`, so the rendered switch remained 34px tall.
- The 17px thumb stayed at `top: 2px`, making it look vertically misaligned.

Fix:
- Override the switch with `min-height: 23px`.
- Vertically center the thumb with `top: 50%` and `translateY(-50%)`.
- Preserve the horizontal ON transition while keeping the thumb centered.
- Add `overflow: hidden`, `line-height: 0`, and `vertical-align: middle` for stable rendering.

Validation:
- `tsc -b`: passed
- `vite build`: passed
- ZIP integrity: checked

Backend was not changed.
