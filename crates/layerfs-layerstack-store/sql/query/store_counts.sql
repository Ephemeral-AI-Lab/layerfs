-- family: query
-- name: store_counts
-- parameters: none
-- results: object_count, commit_count, branch_count, layer_stack_count, layer_count
SELECT
    (SELECT count(*) FROM objects),
    (SELECT count(*) FROM commits),
    (SELECT count(*) FROM branches),
    (SELECT count(*) FROM layer_stacks),
    (SELECT count(*) FROM layers);
