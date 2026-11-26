-- Positions one window of unpositioned children of parent ?2 (#818 I2b), as
-- place_entry.sql does one at a time. Names and positions in the window are
-- distinct; UPDATE ... FROM cannot return the input's columns, so a
-- returned row is matched to its child by its new position. NULL names pad
-- the unused slots.
WITH input(child_name,child_position,child_canonical) AS (VALUES
 (?3,?4,?5),
 (?6,?7,?8),
 (?9,?10,?11),
 (?12,?13,?14),
 (?15,?16,?17),
 (?18,?19,?20),
 (?21,?22,?23),
 (?24,?25,?26),
 (?27,?28,?29),
 (?30,?31,?32),
 (?33,?34,?35),
 (?36,?37,?38),
 (?39,?40,?41),
 (?42,?43,?44),
 (?45,?46,?47),
 (?48,?49,?50),
 (?51,?52,?53),
 (?54,?55,?56),
 (?57,?58,?59),
 (?60,?61,?62),
 (?63,?64,?65),
 (?66,?67,?68),
 (?69,?70,?71),
 (?72,?73,?74),
 (?75,?76,?77),
 (?78,?79,?80),
 (?81,?82,?83),
 (?84,?85,?86),
 (?87,?88,?89),
 (?90,?91,?92),
 (?93,?94,?95),
 (?96,?97,?98)
)
UPDATE init_entry AS target SET position=input.child_position,canonical_position=input.child_canonical,host_identity=NULL
FROM input
WHERE target.operation_id=?1 AND target.parent_position=?2 AND target.name=input.child_name
 AND target.position IS NULL
RETURNING position,kind
