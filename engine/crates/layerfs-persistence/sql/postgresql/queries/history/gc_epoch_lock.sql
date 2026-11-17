-- A removal marker's lock on the project epoch. `FOR UPDATE`, not `FOR
-- SHARE`: the same marker bumps the row later in its transaction, and two
-- concurrent removals in one project upgrading a shared lock would deadlock.
-- The exclusive lock taken here serializes them on the row, so the loser waits
-- for the winner's commit and then sees the bumped value instead of failing.
SELECT epoch FROM project_epoch WHERE layer_stack_id = $1 FOR UPDATE;
