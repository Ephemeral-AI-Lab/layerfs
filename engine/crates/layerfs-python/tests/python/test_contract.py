from support import ContractCase, contains, load_cases, plain


class PythonP6ContractTests(ContractCase):
    async def test_shared_cases(self):
        cases = load_cases()
        from layerfs import LayerFSError

        for case in cases:
            with self.subTest(case=case["name"]):
                await self.native.reset()
                await self.native.prepare(case["prepare"])
                before = await self.native.inspect()
                self.native.refuse = tuple(case["refuse"])
                try:
                    error = None
                    try:
                        result = await self.invoke(case["call"])
                    except LayerFSError as thrown:
                        error = thrown
                    after = await self.native.inspect()
                    calls = after["calls"][len(before["calls"]) :]
                    args = after["args"][len(before["args"]) :]
                    if case["call"]["path"] == "LayerFS.init":
                        keep = [
                            index
                            for index, operation in enumerate(calls)
                            if not (
                                operation == "project.list_branches"
                                and args[index].get("pageSize") == 1
                            )
                        ]
                        calls = [calls[index] for index in keep]
                        args = [args[index] for index in keep]
                    self.assertEqual(calls, [case["op"]] if case["op"] else [])
                    if case["op"]:
                        self.assertTrue(
                            contains(args[0], case["sent"]),
                            f"sent {args[0]!r}, wanted subset {case['sent']!r}",
                        )
                    if case["refused"]:
                        self.assertIsNotNone(error)
                        self.assertEqual(error.code, case["refused"]["code"])
                        self.assertEqual(error.message, case["refused"]["message"])
                        self.assertEqual(plain(error.detail), case["refused"]["detail"])
                    else:
                        self.assertIsNone(error)
                        self.assertEqual(plain(result), case["result"])
                finally:
                    self.native.refuse = ()


if __name__ == "__main__":
    import unittest

    unittest.main()
