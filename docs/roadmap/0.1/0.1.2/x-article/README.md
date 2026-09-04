# LayerFS v0.1.2 X Article editions

- [English article](en/article.md) and [publication replies](en/replies.md)
- [简体中文文章](zh-CN/article.md) and [发布回复串](zh-CN/replies.md)

Each edition includes two architecture diagrams, three statistical charts and
one Big-O table. Regenerate all localized 1600×900 PNGs with:

```sh
python3 docs/roadmap/0.1/0.1.2/x-article/generate_images.py
```

The Cloudflare material is a post-release, informational comparison against the
pinned `de87919` source. It is not part of the immutable v0.1.2 tag or release
admission evidence, and its distinct API/timing boundaries must remain visible
when publishing excerpts.
