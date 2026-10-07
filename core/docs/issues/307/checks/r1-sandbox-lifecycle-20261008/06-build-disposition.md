# Original build disposition

03 Sandbox --locked --no-run PASS. 05 combined build FAILED source type mismatch (Duration reference vs value), fixed from compiler evidence. 05 was inadvertently invoked without --locked while introducing the existing SDK-to-Sandbox path edge; it is not credited as a locked check. Lock delta is exactly one existing first-party dependency name, no new third-party package/version. Final checks use --locked; original raw receipt retained.
