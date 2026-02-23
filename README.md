# gode-check
A CLI that checks the hashes of Geode mods, comparing GitHub releases with workflow artifacts.\
(Originally written in JavaScript by [Prevter](https://github.com/Prevter), rewritten in Rust by [hiimjasmine00](https://github.com/hiimjasmine00).)

## Usage
```bash
gode-check <release link>
```

Optionally, you can set `GODE_CHECK_GITHUB_TOKEN` to fetch GitHub requests authenticated for a higher rate limit.

## License
This project is licensed under the [MIT License](./LICENSE).
