# Worktree setup

[Back to the README](../README.md) · [Configuration reference](configuration.md)

`ezm` discovers existing Git worktrees; it does not create them. For the default five-slot layout, give the worktree directories basenames ending in `-1` through `-5`.

## Working setup

This example starts in `~/src/my-project`, keeps the main checkout there, and creates five sibling worktrees:

```bash
cd ~/src/my-project
git worktree add ../my-project-1 -b ezm/slot-1
git worktree add ../my-project-2 -b ezm/slot-2
git worktree add ../my-project-3 -b ezm/slot-3
git worktree add ../my-project-4 -b ezm/slot-4
git worktree add ../my-project-5 -b ezm/slot-5
ezm
```

The resulting `my-project-1` through `my-project-5` paths are assigned to slots 1 through 5. Branch names are only examples; the directory basename is what controls eligibility. Run `git worktree list --porcelain` to inspect the paths that ezm will read.

## Eligibility and ordering

The discovery path follows these rules:

1. Read paths from `git worktree list --porcelain` and add the current project directory.
2. Canonicalize paths when possible and de-duplicate them.
3. Always keep the current project directory.
4. For every other path, keep it only when its **basename** ends exactly in `-1`, `-2`, `-3`, `-4`, or `-5`, and its basename does not start with `beads` or contain `beads-sync`. These checks are case-sensitive.
5. Sort recognized suffixes numerically from 1 to 5; paths with the same suffix sort lexicographically by path. Paths without a recognized suffix sort after suffixed paths, so the project directory normally comes last. If the project directory itself ends in `-1` through `-5`, it participates in that suffix ordering.

The first five paths after sorting are assigned to slots 1 through 5. If fewer than five paths remain, the final path is reused for the unfilled slots. A directory such as `my-project-6` or `release-preview` is ignored; `beads-main` and `feature-beads-sync-copy` are also ignored unless one is the current project directory.

Use `ezm --no-worktrees` when every slot should deliberately use the current directory instead.
