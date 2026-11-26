-- The first slot of one placement window (#818 I2b) whose row a stored row
-- could refuse: a bound child whose position is a stored identity's
-- canonical position, or any child at a stored directory's position. NULL
-- positions pad the unused slots of the fixed 32-row input.
WITH input(slot,position,host) AS (VALUES
 (0,?2,?3),
 (1,?4,?5),
 (2,?6,?7),
 (3,?8,?9),
 (4,?10,?11),
 (5,?12,?13),
 (6,?14,?15),
 (7,?16,?17),
 (8,?18,?19),
 (9,?20,?21),
 (10,?22,?23),
 (11,?24,?25),
 (12,?26,?27),
 (13,?28,?29),
 (14,?30,?31),
 (15,?32,?33),
 (16,?34,?35),
 (17,?36,?37),
 (18,?38,?39),
 (19,?40,?41),
 (20,?42,?43),
 (21,?44,?45),
 (22,?46,?47),
 (23,?48,?49),
 (24,?50,?51),
 (25,?52,?53),
 (26,?54,?55),
 (27,?56,?57),
 (28,?58,?59),
 (29,?60,?61),
 (30,?62,?63),
 (31,?64,?65)
)
SELECT min(slot) FROM input AS source WHERE position IS NOT NULL AND (
 (host IS NOT NULL AND EXISTS(SELECT 1 FROM init_host_file WHERE operation_id=?1 AND canonical_position=source.position))
 OR EXISTS(SELECT 1 FROM init_entry WHERE operation_id=?1 AND kind=2 AND position=source.position)
)
