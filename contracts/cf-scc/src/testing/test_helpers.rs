pub fn check_equal_vec<S: PartialEq>(v1: Vec<S>, v2: Vec<S>) -> bool {
    v1.len() == v2.len() && v1.iter().all(|x| v2.contains(x)) && v2.iter().all(|x| v1.contains(x))
}