The narrow candidate is implemented: **only `sqlite::pool::group_for` uses existing `prepare_cached`**, with unchanged SQL, parameters, row validation and errors. Production LOC85,723→85,722 (delta−1); no new telemetry, data cache, policy or dependency.

The public authorizer regression passes: baseline five SELECT preparations for five helper queries; candidate first five queries use one preparation, and a sixth query after absence→insert still uses that statement and sees the new row. Updated digest, changed ordinal, missing/range errors and invalid-ordinal precheck are also covered. Independent source review found no blocker.

Unprofiled baseline stride10 completed at23,491,957,417ns operation. Candidate launch was deferred once because another Cargo process was present; no sample consumed and no process interrupted. That observation remains on disk. Candidate measurement is now running under the unchanged protocol. The profile's timings are excluded from this pair.
