// dndtree_wrapper.h

#pragma once
#include <memory>
#include "dndtree.h"
#include "rust/cxx.h"

void set_cpp_trace(bool enable);

class CPPDNDTree {
public:
    std::unique_ptr<DNDTree> inner;

    int insert_edge(int u, int v) const;
    int delete_edge(int u, int v) const;
    bool query(int u, int v) const;
    int get_dsu_root(int u) const;
    int get_tree_parent(int u) const;
    int get_subtree_size(int u) const;
};

std::unique_ptr<CPPDNDTree> new_cpp_dndtree_from_flat_adj(
    int32_t n_nodes,
    rust::Slice<const int32_t> degrees,
    rust::Slice<const int32_t> flat_neighbors,
    bool use_union_find
);