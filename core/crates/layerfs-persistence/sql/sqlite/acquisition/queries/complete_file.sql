-- One fixed 32-row input window; NULL positions pad its unused slots.
WITH roots(position,root) AS (VALUES
 (?2,?3),
 (?4,?5),
 (?6,?7),
 (?8,?9),
 (?10,?11),
 (?12,?13),
 (?14,?15),
 (?16,?17),
 (?18,?19),
 (?20,?21),
 (?22,?23),
 (?24,?25),
 (?26,?27),
 (?28,?29),
 (?30,?31),
 (?32,?33),
 (?34,?35),
 (?36,?37),
 (?38,?39),
 (?40,?41),
 (?42,?43),
 (?44,?45),
 (?46,?47),
 (?48,?49),
 (?50,?51),
 (?52,?53),
 (?54,?55),
 (?56,?57),
 (?58,?59),
 (?60,?61),
 (?62,?63),
 (?64,?65)
)
UPDATE init_native_file AS target SET file_root=roots.root
FROM roots
WHERE target.operation_id=?1 AND target.canonical_position=roots.position
 AND target.file_root IS NULL
RETURNING canonical_position
