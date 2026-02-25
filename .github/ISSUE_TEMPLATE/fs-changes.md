---
title: "std::fs changes detected on {{ date | date('YYYY-MM-DD') }}"
labels: upstream-changes, rust-std-fs
assignees: amacdonald 
---

## Changes Detected in rust-lang/rust std::fs

Changes were detected in `library/std/src/fs.rs` in the upstream Rust repository.

### Commit Hashes

The following commits modified the file in the last 24 hours:

{{ env.HASHES }}

### Links

View the commits on GitHub:
{% assign hashes_array = env.HASHES | split: '\n' %}
{% for hash in hashes_array %}
- https://github.com/rust-lang/rust/commit/{{ hash }}
{% endfor %}

### Action Required

Please review these changes to determine if any updates are needed in our codebase.

---
*This issue was automatically created by the [Check rust-lang/rust std::fs changes workflow]({{ env.GITHUB_SERVER_URL }}/{{ env.GITHUB_REPOSITORY }}/actions)*
