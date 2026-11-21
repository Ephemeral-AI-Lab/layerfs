# DSH Home, navigation and Doctor

These files carry the Home, Center navigation, Doctor and guide changes from
DSH commit `e787f7d46370b2ea5a66dede94c796f3c9e24530`. Paths below this
directory match the standalone [DSH plugin repository](https://github.com/Ephemeral-AI-Lab/dsh-layerfs).

Apply the files to a clean DSH checkout at commit
`bee7be2c6dea5d4975b1aee538e489f75350b45c`. From that checkout, set
`dsh_overlay` to this directory's absolute path and copy the five folders.

```sh
cp -R "$dsh_overlay/docs" "$dsh_overlay/scripts" "$dsh_overlay/src" "$dsh_overlay/test" "$dsh_overlay/tools" .
```

The DSH build and type check passed with 870 tests passing and one legacy
CLI case skipped. Chromium checks covered Home, Center Add and Jump, the
sidebar, refresh, and the bilingual guide. Doctor contract parity and
native Sandbox mounting and teardown were verified against LayerFS commit
`41788e2c258b9a42c56d96306fa9980d55b29545`.
