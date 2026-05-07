use git2::{Repository, Oid};

fn main() {
    let repo = Repository::open("/Users/poseidomhung/Documents/github/Infinity/Ark").unwrap();
    let head = repo.head().unwrap();
    let upstream = repo.branch_upstream_name(head.name().unwrap()).unwrap();
    let u_oid = repo.find_reference(upstream.as_str().unwrap()).unwrap().target().unwrap();
    
    let older_oid = Oid::from_str("0478c9344f47bf2506319b90bb3aaca612bbae43").unwrap();
    println!("descendant(u_oid, older_oid): {:?}", repo.graph_descendant_of(u_oid, older_oid));
}
