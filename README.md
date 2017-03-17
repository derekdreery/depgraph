# depgraph

Makefile-style build of native stuff, for use in build.rs. Checks modified
times on output and input files, and then runs operation if there are changes
to input files.

# Example

This example builds object files from assembly using yasm when the assembly
files change.

```rust
extern crate depgraph;

use depgraph;

fn build_assembly(out: &str, deps: &[&str]) -> Result<(), String> {
    // Make sure the folder we're going to output to exists.
    let out_dir = Path::new(out).parent().unwrap();
    fs::create_dir_all(out_dir).unwrap();

    // Run the command with correct argument order
    Command.new("yasm").args(&["-f", "elf64", "-o"]).arg(out).args(deps)
        .status().unwrap();
    // Everything went ok so we return Ok(()). Instead of panicking, we could
    // have returned an error message and handled it in main.
    Ok(())
}

fn main() {
    // Get the directory we should put files in.
    let out_dir = Path::new(env::var("OUT_DIR").unwrap());
    // Create the graph builder
    let mut graph = depgraph::DepGraphBuilder::new();
    // Add a rule to build an object file from an asm file using the build
    // script in `build_assembly`.
    graph.add_rule(out_dir.join("out/path/file.o").to_str().unwrap(),
        &["src/input_file.asm"],
        build_assembly);
    // Build the graph, internally this checks for cyclic dependencies.
    let graph = graph.build().unwrap();
    // Run the necessary build scripts in the correct order.
    graph.make(false).unwrap();
}
```

# TODO

 1. Preserve dependency order
 2. Automated tests
 3. More generics (not sure if this would add anything)
 3. Optimizations (again not sure this would add anything)
