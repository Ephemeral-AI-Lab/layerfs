"""Tests for store_blocks.py: the engine's golden blocks (engine tests
sqlite_inode_values.rs and binding_hint_blocks.rs assert the same bytes),
round trips of every value shape, and checksum refusal."""

import random
import unittest

import store_blocks as sb

GOLDEN_VALUES = bytes.fromhex(
    "010402000081a4000081ed810111111111111111111111111111111111111111111111111111111111111111110080c49fd50c0102030481011111111111111111111111111111111111111111111111111111111111111111010a01020304010222222222222222222222222222222222222222222222222222222222222222223333333333333333333333333333333333333333333333333333333333333333fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff2f3427f"
)
GOLDEN_HINTS = [
    (0, bytes.fromhex("010307020005612e74787400000005622e74787402020005612e7478742d1e97ab")),
    (1, bytes.fromhex("0101060200017860fbeefb")),
]


def inline(mode, mtime, refs=1):
    head = bytes([0x81]) + refs.to_bytes(8, "big") + bytes([0x11]) * 32 + mode.to_bytes(4, "big")
    return head + mtime.to_bytes(8, "big", signed=True) + bytes([1, 2, 3, 4]) + bytes(16)


GOLDEN_ROWS = [
    inline(0o100644, 1_700_000_000),
    inline(0o100755, 1_700_000_005),
    bytes([1]) + (2).to_bytes(8, "big") + bytes([0x22]) * 32 + bytes([0x33]) * 32,
    bytes([0xFF]) * 73,
]
HINTS = [(7, 1, b"a.txt"), (7, 1, b"b.txt"), (9, 2, b"a.txt"), (70, 1, b"x")]


class StoreBlocksTest(unittest.TestCase):
    def test_golden_value_block(self):
        self.assertEqual(sb.decode_values(5, 8, GOLDEN_VALUES), GOLDEN_ROWS)
        self.assertEqual(sb.encode_values(list(zip(range(5, 9), GOLDEN_ROWS))), [(5, 8, GOLDEN_VALUES)])

    def test_golden_hint_blocks(self):
        self.assertEqual(sb.encode_hints(HINTS + HINTS[:1]), GOLDEN_HINTS)
        decoded = [h for g, body in GOLDEN_HINTS for h in sb.decode_hints(g, body)]
        self.assertEqual(decoded, HINTS)

    def test_every_value_shape_round_trips(self):
        rng = random.Random(91)
        rows = []
        for ordinal in range(1, 2000):
            value = bytearray(rng.randbytes(73))
            shape = ordinal % 4
            if shape == 0:
                value[0] = 0x80 | rng.randrange(1, 0x7F)
                value[57:] = bytes(16)
            elif shape == 1:
                value[0] &= 0x7F
            if ordinal % 97 == 0:
                continue  # a gap starts a new block
            rows.append((ordinal, bytes(value)))
        blocks = sb.encode_values(rows)
        self.assertTrue(all(len(body) <= sb.BODY_LIMIT for _, _, body in blocks))
        decoded = [
            (first + i, v) for first, last, body in blocks for i, v in enumerate(sb.decode_values(first, last, body))
        ]
        self.assertEqual(decoded, rows)

    def test_a_changed_byte_fails_the_checksum(self):
        body = bytearray(GOLDEN_VALUES)
        body[10] ^= 1
        with self.assertRaises(ValueError):
            sb.decode_values(5, 8, bytes(body))
        with self.assertRaises(ValueError):
            sb.decode_values(5, 9, GOLDEN_VALUES)


if __name__ == "__main__":
    unittest.main()
