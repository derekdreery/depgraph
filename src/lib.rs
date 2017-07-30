//! A library to help build dependencies externally from rust. Uses petgraph under the hood for
//! managing the graph structure.
//!
//! # Example
//! *An example is worth a thousand words* - made up quote.
//!
//! ## Example build script
//!
//! ```no_run
//! extern crate depgraph;
//! use std::path::Path;
//! use std::{fs, env};
//! use std::process::Command;
//!
//! fn build_assembly(out: &Path, deps: &[&Path]) -> Result<(), String> {
//!     // Make sure the folder we're going to output to exists.
//!     let out_dir = out.parent().unwrap();
//!     fs::create_dir_all(out_dir).unwrap();
//!
//!     // Run the command with correct argument order
//!     Command::new("yasm").args(&["-f", "elf64", "-o"]).arg(out).args(deps)
//!         .status().unwrap();
//!     // Everything went ok so we return Ok(()). Instead of panicking, we could
//!     // have returned an error message and handled it in main.
//!     Ok(())
//! }
//!
//! fn main() {
//!     // Get the directory we should put files in.
//!     let out_dir = env::var("OUT_DIR").unwrap();
//!     let out_dir = Path::new(&out_dir);
//!     // Create the graph builder
//!     let graph = depgraph::DepGraphBuilder::new()
//!     // Add a rule to build an object file from an asm file using the build
//!     // script in `build_assembly`.
//!       .add_rule(out_dir.join("out/path/file.o"),
//!                 &[Path::new("src/input_file.asm")],
//!                 build_assembly)
//!     // Build the graph, internally this checks for cyclic dependencies.
//!       .build().unwrap();
//!     // Run the necessary build scripts in the correct order.
//!     graph.make(depgraph::MakeParams::None).unwrap();
//! }
//! ```
//!


extern crate petgraph;
#[cfg(test)]
extern crate tempdir;

mod error;

use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::fmt;

use petgraph::Graph;
use petgraph::graph::NodeIndex;

#[cfg(feature = "petgraph_visible")]
pub use petgraph;

pub use error::{Error, DepResult};

/// (Internal) Information on a dependency (how to build it and what it's called)
///
/// TODO keep copy of dependencies in order, so we don't have to look them up on the graph, and
/// they stay in order
struct DependencyNode {
    filename: PathBuf,
    build_fn: Option<Box<Fn(&Path, &[&Path]) -> Result<(), String>>>,
}

impl fmt::Debug for DependencyNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DependencyNode(\"{:?}\")", self.filename)
    }
}

/// Used to construct a DepGraph
///
/// See the module level documentation for an example of how to use this
pub struct DepGraphBuilder {
    /// List of edges, .0 is dependent, .1 is dependencies, .2 is build fn
    edges: Vec<(PathBuf, Vec<PathBuf>, Box<Fn(&Path, &[&Path]) -> Result<(), String>>)>,
}

impl DepGraphBuilder {
    /// Create a `DepGraphBuilder` with no rules.
    pub fn new() -> DepGraphBuilder {
        DepGraphBuilder { edges: Vec::new() }
    }

    /// Add a new rule (a file with its dependent files and build instructions).
    ///
    /// These can be added in any order, and can be chained.
    pub fn add_rule<F, P1, P2>(
        mut self,
        filename: P1,
        dependencies: &[P2],
        build_fn: F,
    ) -> DepGraphBuilder
    where
        F: Fn(&Path, &[&Path]) -> Result<(), String> + 'static,
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        self.edges.push((
            filename.as_ref().to_path_buf(),
            dependencies
                .iter()
                .map(|s| s.as_ref().to_path_buf())
                .collect(),
            Box::new(build_fn),
        ));
        self
    }

    /// Build the make graph and check for errors like cyclic dependencies and duplicate files.
    pub fn build(self) -> DepResult<DepGraph> {
        // used to check a file isn't added more than once. (filename -> NodeId)
        let mut files = HashMap::new();
        // used between passes to store edges
        let mut edges_after_node = Vec::with_capacity(self.edges.len());
        // the resulting graph
        let mut graph = Graph::new();

        // Job of first iteration is to add nodes and save ids for them
        for edge in self.edges.into_iter() {
            let (filename, dependencies, build_fn) = edge;
            // error if file already added
            if files.contains_key(&filename) {
                return Err(Error::DuplicateFile);
            }
            // add node to graph and get index
            let idx = graph.add_node(DependencyNode {
                filename: filename.clone(),
                build_fn: Some(build_fn),
            });
            // add file to list
            files.insert(filename, idx);
            edges_after_node.push((idx, dependencies));
        }

        // Job of second iteration is to add in edges using `edges_after_node` and add in leaves
        // for files not found elsewhere
        for edge in edges_after_node.into_iter() {
            let (idx, dependencies) = edge;
            for dep in dependencies.into_iter() {
                // value is just number so deref to copy it
                let maybe_dep = files.get(&dep).map(|v| *v);
                if let Some(idx2) = maybe_dep {
                    // file already a dependency, so add directed edge from file to it's dependency
                    graph.add_edge(idx, idx2, ());
                } else {
                    // file not yet a dependency - add it
                    let idx2 = graph.add_node(DependencyNode {
                        filename: dep.clone(),
                        build_fn: None,
                    });
                    files.insert(dep, idx2);
                    graph.add_edge(idx, idx2, ());
                }
            }
        }

        if petgraph::algo::is_cyclic_directed(&graph) {
            return Err(Error::Cycle);
        }

        Ok(DepGraph {
            graph: graph,
            //file_hash: files,
        })
    }
}

/// Contains the checked and parsed dependency graph, ready for execution (`fn make`)
pub struct DepGraph {
    /// Node is file (weight is filename, build function), edge is dependency
    graph: Graph<DependencyNode, ()>,
    //file_hash: HashMap<String, NodeIndex<u32>>,
}

/// When running the build scripts, we can either only build when output files are newer than their
/// dependencies, or we can force the build script to run regardless. This enum allows for those
/// two choices.
#[derive(Debug, Clone, Copy)]
pub enum MakeParams {
    /// Just build normally, where we only rebuild if the source was updated
    None,
    /// Always build, regardless of status of source
    ForceBuild,
}

impl DepGraph {
    /// Run the build
    ///
    /// If force is true, all build functions will be run, regardless of file times, otherwise
    /// build will only be run if one of the dependency files is newer than the output file.
    // There are possible optimizations here as there are redundent metadata checks, I don't think
    // this is a big deal though.
    pub fn make(&self, make_params: MakeParams) -> DepResult<()> {
        // Get files in dependency order
        // Needs to be reversed to build in right order
        let ordered_deps_rev = petgraph::algo::toposort(&self.graph, None).map_err(
            |_| Error::Cycle,
        )?;
        let force: bool = match make_params {
            MakeParams::None => false,
            MakeParams::ForceBuild => true,
        };
        for node in ordered_deps_rev.iter().rev() {
            self.build_dependency(*node, force)?;
        }
        Ok(())
    }

    /// Helper function to build a specific dependency
    fn build_dependency(&self, idx: NodeIndex<u32>, force: bool) -> DepResult<()> {
        let dep = self.graph.node_weight(idx).unwrap();
        // collect names of children (don't copy strings)
        let children: Vec<&Path> = self.graph
            .neighbors_directed(idx, petgraph::Outgoing)
            .map(|idx| {
                self.graph.node_weight(idx).unwrap().filename.as_path()
            })
            .collect();
        for child in children.iter() {
            if !Path::new(child).exists() {
                return Err(Error::MissingFile((*child).to_owned()));
            }
        }
        // if there is a build script, and dependency timestamps are newer, run it
        if let Some(ref f) = dep.build_fn {
            if force || dependencies_newer(&dep.filename, &children) {
                f(&dep.filename, &children).map_err(
                    |s| Error::BuildFailed(s),
                )?;
            }
        }
        // check that file has been created
        if Path::new(&dep.filename).exists() {
            Ok(())
        } else {
            Err(Error::MissingFile(dep.filename.clone()))
        }
        //println!("{:?}", children);
    }

    /// Get the underlying graph
    #[cfg(feature = "petgraph_visible")]
    pub fn into_inner(self) -> (Graph<DependencyNode, ()>, HashMap<String, NodeIndex<u32>>) {
        (self.graph, self.file_hash)
    }
}

/// Checks if any of the files in the dependency list are newer than the file given by `filename`.
fn dependencies_newer(filename: &Path, deps: &[&Path]) -> bool {
    if !filename.exists() {
        return true;
    }
    let file_mod_time = fs::metadata(filename).unwrap().modified().unwrap();
    for dep in deps {
        let dep_mod_time = fs::metadata(Path::new(dep)).unwrap().modified().unwrap();
        if dep_mod_time > file_mod_time {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::io::{Write, Read};
    use std::fs::File;
    use std::io;
    use super::*;
    use tempdir::TempDir;

    fn copy_build(fname: &Path, deps: &[&Path]) -> Result<(), String> {
        fn io_err_to_string(err: io::Error) -> String {
            err.to_string()
        }
        let mut out = File::create(fname).map_err(io_err_to_string)?;
        for d in deps {
            let mut in_f = File::open(d).map_err(io_err_to_string)?;
            let mut buf = String::new();
            in_f.read_to_string(&mut buf).map_err(io_err_to_string)?;
            write!(&mut out, "{}", buf).map_err(io_err_to_string)?;
        }
        Ok(())
    }

    #[test]
    fn smoke_test() {
        let tmp_dir = TempDir::new("depgraph-tests").unwrap();
        let tmp = tmp_dir.path();
        println!("tmp dir {:?}", tmp);
        let makegraph = DepGraphBuilder::new()
            .add_rule(
                tmp.join("File1"),
                &[tmp.join("file2"), tmp.join("file3")],
                copy_build,
            )
            .add_rule(tmp.join("file2"), &[tmp.join("file3")], copy_build)
            .add_rule(tmp.join("file4"), &[tmp.join("file5")], copy_build)
            .build()
            .unwrap();
        {
            let mut file3 = File::create(tmp.join("file3")).unwrap();
            write!(&mut file3, "file3\n").unwrap();

            let mut file5 = File::create(tmp.join("file5")).unwrap();
            write!(&mut file5, "file5\n").unwrap();
        }
        makegraph.make(MakeParams::None).unwrap();
    }
}
