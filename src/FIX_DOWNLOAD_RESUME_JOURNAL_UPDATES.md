# Download/resume/journal/update-list fix

Applied to the uploaded frontend baseline `src(12).zip`.

## Frontend changes

- Download speed is now calculated from a rolling 10-second byte/time window instead of a near-instantaneous sample.
- ETA uses the same stabilized moving average.
- Speed history resets when the job pauses, stops, is canceled, or changes job ID.
- Uninstall first cancels and cleans any job owned by that game.
- The Updates page only receives games that are installed and whose installed version differs from the catalog's latest version.
- The Updates badge shows the actual number of update-ready installed games.
- Empty Updates view now explicitly reports that there are no updates.

## Validation

- `tsc -b`: passed.
- `vite build`: passed.
- The two existing font URL warnings remain because the uploaded source archive does not include those font files.
