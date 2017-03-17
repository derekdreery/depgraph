//! A library to help build dependencies externally from rust. Uses petgraph under the hood for
//! managing the graph structure.
//!
//! # Example
//! *An example is worth a thousand words* - made up quote.
//!
//! ## Example build script
//!
//! ```rust
//! extern crate depgraph;
//!
//! use depgraph;
//!
//! fn build_assembly(out: &str, deps: &[&str]) -> Result<(), String> {
//!     // Make sure the folder we're going to output to exists.
//!     let out_dir = Path::new(out).parent().unwrap();
//!     fs::create_dir_all(out_dir).unwrap();
//!
//!     // Run the command with correct argument order
//!     Command.new("yasm").args(&["-f", "elf64", "-o"]).arg(out).args(deps)
//!         .status().unwrap();
//!     // Everything went ok so we return Ok(()). Instead of panicking, we could
//!     // have returned an error message and handled it in main.
//!     Ok(())
//! }
//!
//! fn main() {
//!     // Get the directory we should put files in.
//!     let out_dir = Path::new(env::var("OUT_DIR").unwrap());
//!     // Create the graph builder
//!     let mut graph = depgraph::DepGraphBuilder::new();
//!     // Add a rule to build an object file from an asm file using the build
//!     // script in `build_assembly`.
//!     graph.add_rule(out_dir.join("out/path/file.o").to_str().unwrap(),
//!         &["src/input_file.asm"],
//!         build_assembly);
//!     // Build the graph, internally this checks for cyclic dependencies.
//!     let graph = graph.build().unwrap();
//!     // Run the necessary build scripts in the correct order.
//!     graph.make(false).unwrap();
//! }
//! ```
//!


extern crate petgraph;

mod error;

use std::fs;
use std::path::{Path};
use std::collections::{HashMap};
use std::fmt;

use petgraph::{Graph};
use petgraph::graph::{NodeIndex};

#[cfg(feature = "petgraph_visible")]
pub use petgraph;

pub use error::{Error, DepResult};

/// (Internal) Information on a dependency (how to build it and what it's called)
///
/// TODO keep copy of dependencies in order, so we don't have to look them up on the graph, and
/// they stay in order
struct DependencyNode {
    filename: String,
    build_fn: Option<Box<Fn(&str, &[&str]) -> Result<(), String>>>,
}

impl fmt::Debug for DependencyNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DependencyNode(\"{}\")", self.filename)
    }
}

/// Used to construct a DepGraph
///
/// See the module level documentation for an example of how to use this
pub struct DepGraphBuilder {
    /// List of edges, .0 is dependent, .1 is dependencies, .2 is build fn
    edges: Vec<(String, Vec<String>, Box<Fn(&str, &[&str]) -> Result<(), String>>)>
}

impl DepGraphBuilder {
    /// Create a `DepGraphBuilder` with no rules.
    pub fn new() -> DepGraphBuilder {
        DepGraphBuilder { edges: Vec::new() }
    }

    /// Add a new rule (a file with its dependent files and build instructions).
    ///
    /// These can be added in any order, and can be chained.
    pub fn add_rule<F>(&mut self, filename: &str, dependencies: &[&str], build_fn: F)
        -> &DepGraphBuilder
        where F: Fn(&str, &[&str]) -> Result<(), String> + 'static
    {
        self.edges.push((
            filename.into(),
            dependencies.iter().map(|s| (*s).into()).collect(),
            Box::new(build_fn)
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
                return Err(Error::DuplicateFile)
            }
            // add node to graph and get index
            let idx = graph.add_node(DependencyNode {
                filename: filename.clone(),
                build_fn: Some(build_fn)
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
                        build_fn: None
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

impl DepGraph {
    /// Run the build
    ///
    /// If force is true, all build functions will be run, regardless of file times, otherwise
    /// build will only be run if one of the dependency files is newer than the output file.
    // There are possible optimizations here as there are redundent metadata checks, I don't think
    // this is a big deal though.
    pub fn make(&self, force: bool) -> DepResult<()> {
        // Get files in dependency order
        // Needs to be reversed to build in right order
        let ordered_deps_rev = petgraph::algo::toposort(&self.graph, None)
            .map_err(|_| Error::Cycle)?;

        for node in ordered_deps_rev.iter().rev() {
            self.build_dependency(*node, force)?;
        }
        Ok(())
    }

    /// Helper function to build a specific dependency
    fn build_dependency(&self, idx: NodeIndex<u32>, force: bool) -> DepResult<()> {
        let dep = self.graph.node_weight(idx).unwrap();
        // collect names of children (don't copy strings)
        let children: Vec<&str> = self.graph.neighbors_directed(idx, petgraph::Outgoing)
            .map(|idx| self.graph.node_weight(idx).unwrap().filename.as_str())
            .collect();
        for child in children.iter() {
            if ! Path::new(child).exists() {
                return Err(Error::MissingFile((*child).to_owned()))
            }
        }
        // if there is a build script, and dependency timestamps are newer, run it
        if let Some(ref f) = dep.build_fn {
            if force || dependencies_newer(&dep.filename, &children) {
                f(&dep.filename, &children).map_err(|s| Error::BuildFailed(s))?;
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
    pub fn into_inner(self)
        -> (Graph<DependencyNode, ()>, HashMap<String, NodeIndex<u32>>)
    {
        (self.graph, self.file_hash)
    }
}

/// Checks if any of the files in the dependency list are newer than the file given by `filename`.
fn dependencies_newer(filename: &str, deps: &[&str]) -> bool {
    let filename = Path::new(filename);
    if ! filename.exists() {
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
    use super::*;

    fn fake_build(fname: &str, deps: &[&str]) -> Result<(), String> { Ok(()) }

    #[test]
    fn builder() {
        let mut builder = DepGraphBuilder::new();
        builder.add_rule("File1", &["file2", "file3"], fake_build);
        builder.add_rule("file2", &["file3"], fake_build);
        builder.add_rule("file4", &["file5"], fake_build);
        //builder.add_rule("file5", &["file4"], fake_build);
        let makegraph = builder.build().unwrap();
        makegraph.make(false).unwrap();
        //println!("{:?}", maketree.tree);
        panic!();
    }
}


