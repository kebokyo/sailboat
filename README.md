# sailboat
a little ship in the black sea to help your projects ship

sailboat is a **TUI client for [Plane](https://plane.so)**, a FOSS alternative to other project management suites like Jira.
we use plane all the time at our furry work, but the webapp is slow and native clients leave less to be desired.
we decided to make our own client: one that lives in the terminal &mdash; either in its own window, or inside your favorite
IDE &mdash; to easily track and log progress, next steps, future plans, and more.

> [!IMPORTANT]
> this software is **very work-in-progress**. missing features, bugs, and jank will happily greet you here.
> 
> also note that we make heavy use of Claude Code, specifically the Opus 5 model, to assist with development. we are not
> well-versed in Rust, but we thought it would be good to learn it through making this project. we use "Miss Claude" to
> teach us how to write in Rust, automate boilerplate and repetitive implementations, and troubleshoot bugs. we believe
> that we are not "vibe-coding" but instead asking for the assistance of the global (parasitic) librarian to develop
> this software alongside us. we understand that a potential user might think differently, and since we made this
> software specifically for *our* use and ours alone, we figured being honest about our use of AI would be better than
> saying nothing and getting "caught" resulting in even more shame from others.

## getting started

you'll need Rust 1.95 or newer.

```sh
git clone https://github.com/kebokyo/sailboat
cd sailboat
cargo run --release
```

sailboat needs a Plane API token and your workspace slug. create a personal access token in
your Plane profile settings; the workspace slug is the first path segment of your
Plane URL (`https://app.plane.so/<slug>/...`). the easiest place for both is a `.env` file at the root
of the repo (it's gitignored):

```sh
PLANE_API_KEY=plane_api_...
PLANE_WORKSPACE=your-workspace
# only if you self-host:
# PLANE_BASE_URL=https://plane.example.com
```

you can also put settings in `~/.config/sailboat/config.toml` (or wherever `SAILBOAT_CONFIG` points).
environment variables win over the file.

```toml
[api]
base_url = "https://api.plane.so"

[workspace]
slug = "your-workspace"
```

### keys

| key | does |
|-----|------|
| `j`/`k`, `↓`/`↑` | move up and down |
| `l`, `→`, `Enter` | open the highlighted project or work item; on a work item's details, edit the highlighted attribute |
| `h`, `←`, `Backspace` | go back |
| `n` | on a project's work items, create a new one (asks for a name; everything else starts at the project's defaults) |
| `x` | close the highlighted or open work item (moves it to the project's first completed state) |
| `q`, `Esc` | quit |

name, state, priority and the start and target dates can be edited. in a text field, `Enter` saves,
`Esc` cancels, and an emptied date field clears the date.

## License

Unless otherwise noted, all files in this distribution are released
under the Common Development and Distribution License (CDDL).
Exceptions are noted within the associated source files.