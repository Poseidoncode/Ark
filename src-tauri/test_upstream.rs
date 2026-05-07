use git2::Repository;

fn main() {
    let repo = Repository::open("/Users/poseidomhung/Documents/github/Infinity/Ark").unwrap();
    let head = repo.head().unwrap();
    println!("head is branch: {}", head.is_branch());
    println!("head name: {:?}", head.name());
    
    if let Some(name) = head.name() {
        if let Ok(upstream) = repo.branch_upstream_name(name) {
            println!("upstream name: {:?}", upstream.as_str());
            if let Some(u_name) = upstream.as_str() {
                if let Ok(r) = repo.find_reference(u_name) {
                    println!("upstream target: {:?}", r.target());
                } else {
                    println!("could not find reference {}", u_name);
                }
            }
        } else {
            println!("could not get upstream name for {}", name);
        }
    }
}
