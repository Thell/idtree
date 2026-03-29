#include "dndtree_wrapper.h"
#include <vector>

bool G_TRACE_ENABLED = false;

void set_cpp_trace(bool enable) {
    G_TRACE_ENABLED = enable;
}

int CPPDNDTree::insert_edge(int u, int v) const {
    return inner->insert_edge(u, v);
}

int CPPDNDTree::delete_edge(int u, int v) const {
    return inner->delete_edge(u, v);
}

bool CPPDNDTree::query(int u, int v) const {
    return inner->query(u, v);
}

int CPPDNDTree::get_dsu_root(int u) const {
    if (!inner->use_union_find) return -1;
    // We cast away const because get_f may perform path compression
    return const_cast<DNDTree*>(inner.get())->get_f(u);
}

int CPPDNDTree::get_tree_parent(int u) const {
    if (u < 0 || u >= (int)inner->nodes.size()) return -1;
    return inner->nodes[u].p;
}

int CPPDNDTree::get_subtree_size(int u) const {
    if (u < 0 || u >= (int)inner->nodes.size()) return 0;
    return inner->nodes[u].sub_cnt;
}

std::unique_ptr<CPPDNDTree> new_cpp_dndtree_from_flat_adj(
    int32_t n,
    rust::Slice<const int32_t> degrees,
    rust::Slice<const int32_t> flat_neighbors,
    bool use_union_find
) {
    // 1. Reconstruct the adjacency list from the flat slices
    std::vector<std::vector<int>> adj_list(n);
    size_t offset = 0;

    for (int i = 0; i < n; ++i) {
        int deg = degrees[i];
        if (deg > 0) {
            adj_list[i].reserve(deg);
            for (int j = 0; j < deg; ++j) {
                adj_list[i].push_back(flat_neighbors[offset + j]);
            }
            offset += deg;
        }
    }

    // 2. Create the wrapper object
    auto wrapper = std::make_unique<CPPDNDTree>();
    
    // 3. Initialize the inner DNDTree using the reconstructed adj_list
    wrapper->inner = std::make_unique<DNDTree>(n, adj_list, use_union_find);
    
    return wrapper;
}