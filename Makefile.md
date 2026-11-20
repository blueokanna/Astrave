# Makefile

Run from the repository root. Build artifacts go to `target/`; packages go to `dist/`.

| Command                                 | Description                                              |
|-----------------------------------------|----------------------------------------------------------|
| `make web`                              | Build the server including its Rust-rendered dashboard   |
| `make docs`                             | Generate the bilingual static site in `docs/build/`      |
| `make build`                            | Debug-build `astrave-server` and `astrave`                 |
| `make release`                          | Release-build server and client                          |
| `make astrave-server`                   | Release-build the server                                 |
| `make astrave`                           | Release-build the client                                 |
| `make desktop-dev`                      | Run the desktop app (dev)                                |
| `make desktop-build`                    | Release-build the desktop app                            |
| `make desktop-app` / `make desktop-dmg` | Package macOS `.app` / `.dmg`                            |
| `make desktop-windows`                  | Package Windows portable zip (run on Windows)            |
| `make desktop-deb`                      | Package Linux `.deb` (run on Debian/Ubuntu)              |
| `make package`                          | Run `release`, then copy binaries and `conf/` to `dist/` |
| `make test`                             | Run Rust tests for all package features                  |
| `make fmt`                              | Format Rust code                                         |
| `make clean`                            | Clean Cargo artifacts and the generated documentation    |
