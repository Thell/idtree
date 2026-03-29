
#ifndef DNDTREE_H_
#define DNDTREE_H_

#include <vector>
#include <algorithm> // for sort (adj buf temporal order and init degree order)
#include <iostream>  // for string input

#define MAXDEP 32768 // for insert_edge_balanced

extern bool G_TRACE_ENABLED;

using namespace std;

// MARK: LinkNode

class LinkNode
{
public:
    int v;
    LinkNode *prev;
    LinkNode *next;

public:
    LinkNode();
    void isolate();
};

inline LinkNode::LinkNode()
{
    v = -1;
    prev = NULL;
    next = NULL;
}

inline void LinkNode::isolate()
{
    LinkNode *tmp = prev;
    if (prev)
    {
        prev->next = next;
        prev = NULL;
    }
    if (next)
    {
        next->prev = tmp;
        next = NULL;
    }
}

// MARK: Node

class Node
{
public:
    // for graph
    vector<int> adj;

    // for tree
    int p;       // parent node in the tree
    int sub_cnt; // number of descendants in the tree

    // for union_find
    int f;            // father node in union_find
    LinkNode l_start; // start node in the list for union_find
    LinkNode l_end;   // end node in the list for union_find

public:
    Node();
    void insert_l_node(LinkNode *v);
    void insert_l_nodes(Node *v);

public:
    vector<pair<int, int>> del_buf;
    vector<pair<int, int>> ins_buf;

public:
    void insert_adj(int u);
    void delete_adj(int u);
    void flush();
};

inline Node::Node()
{
    p = -1;
    f = -1;
    sub_cnt = 0;
    l_start.next = &l_end;
    l_start.prev = NULL;
    l_end.prev = &l_start;
    l_end.next = NULL;
}

inline void Node::insert_l_node(LinkNode *v)
{
    v->next = l_start.next;
    v->prev = &l_start;
    l_start.next->prev = v;
    l_start.next = v;
}

inline void Node::insert_l_nodes(Node *v)
{

    if (v->l_start.next == &v->l_end || v == this)
        return;

    LinkNode *s = v->l_start.next, *t = v->l_end.prev;
    t->next = l_start.next;
    s->prev = &l_start;
    l_start.next->prev = t;
    l_start.next = s;

    v->l_start.next = &v->l_end;
    v->l_end.prev = &v->l_start;
}

inline void Node::insert_adj(int u)
{
    ins_buf.push_back(make_pair(u, (int)(ins_buf.size() + del_buf.size())));
}
inline void Node::delete_adj(int u)
{
    del_buf.push_back(make_pair(u, (int)(ins_buf.size() + del_buf.size())));
}
inline void Node::flush()
{
    if (G_TRACE_ENABLED) {
        std::cout << "[CPP]   node::flush" << std::endl;
        std::cout.flush();
    }

    if (ins_buf.size() == 0 && del_buf.size() == 0)
        return;
    sort(ins_buf.begin(), ins_buf.end());
    sort(del_buf.begin(), del_buf.end());

    int i = 0, d = 0, ni = (int)ins_buf.size(), nd = (int)del_buf.size(), td = 0, ti = 0;

    vector<int> l;
    for (int j = 0; j <= (int)adj.size(); ++j)
    {
        int v = j < (int)adj.size() ? adj[j] : INT_MAX;
        for (; i < ni && ins_buf[i].first < v; ++i)
        {
            while (i < ni - 1 && ins_buf[i].first == ins_buf[i + 1].first)
                ++i;
            while (d < nd && del_buf[d].first < ins_buf[i].first)
                ++d;
            while (d < nd && del_buf[d].first == ins_buf[i].first)
                ++d;
            if (d > 0 && del_buf[d - 1].first == ins_buf[i].first && del_buf[d - 1].second > ins_buf[i].second)
                continue;
            l.push_back(ins_buf[i].first);
        }
        if (j == (int)adj.size())
            break;
        while (d < nd && del_buf[d].first < v)
            ++d;
        if (d >= nd || del_buf[d].first > v)
        {
            l.push_back(v);
            while (i < ni && ins_buf[i].first == v)
                ++i;
            continue;
        }
        if (i < ni && ins_buf[i].first == v)
        {
            for (; i < ni && ins_buf[i].first == v; ++i)
                ti = ins_buf[i].second;
            for (; d < nd && del_buf[d].first == v; ++d)
                td = del_buf[d].second;
            if (ti > td)
                l.push_back(v);
        }
    }
    ins_buf.clear();
    del_buf.clear();

    // sort l
    sort(l.begin(), l.end());
    adj = l;
}

// MARK: DNDTree

class DNDTree
{
public:
    int n;
    vector<Node> nodes;
    vector<LinkNode> l_nodes;

public:
    // used in algorithms
    vector<bool> used;
    vector<int> q, l;

public:
    DNDTree(string path, bool load_graph = true, bool use_union_find = true);
    DNDTree(int n_nodes, const std::vector<std::vector<int>>& adj_list, bool use_union_find);

    void init(); // initialize the information in nodes
    bool insert_edge_in_graph(int u, int v);
    bool delete_edge_in_graph(int u, int v);
    int insert_edge(int u, int v); //-1:not inserted; 0:non-tree edge; 1:tree-edge
    int delete_edge(int u, int v); //-1:not inserted; 0:non-tree edge; 1:tree-edge with replacement; 2:tree-edge without replacement

private:
    int insert_edge_balanced(int u, int v);
    int delete_edge_balanced(int u, int v);
    void reroot(int u, int fv);

    bool find_replacement(int u, int f);

public:
    bool use_union_find;

public:
    bool query(int u, int v);

public: // for Algorithm_Union_Find
    int get_f(int u);
    void union_f(int u, int v);
    void remove_subtree_union_find(int u, int v, bool needreroot);
};

inline DNDTree::DNDTree(string path, bool load_graph, bool use_union_find)
{
    this->use_union_find = use_union_find;

    // Change this to use an adjacency list passed in as an argument
    FILE *fin = fopen((path + "graph.bin").c_str(), "rb");
    if (fin == NULL || !load_graph)
    {
        if (fin)
            fclose(fin);
        fin = fopen((path + "graph.stream").c_str(), "rb");
        fread(&n, sizeof(int), 1, fin);
        nodes.resize(n);
        fclose(fin);
        return;
    }

    fread(&n, sizeof(int), 1, fin);
    nodes.resize(n);
    int *deg = new int[n], *dat = new int[n];

    printf("Loading graph...\n");
    long long m = 0;
    fread(deg, sizeof(int), n, fin);
    for (int i = 0; i < n; ++i)
    {
        fread(dat, sizeof(int), deg[i], fin);
        m += deg[i];
        nodes[i].adj.assign(dat, dat + deg[i]);
    }

    delete[] deg;
    delete[] dat;
    fclose(fin);

    this->use_union_find = use_union_find;
    printf("Graph loaded, n = %d, m = %lld\n", n, m / 2);
}

// inline DNDTree::DNDTree(int n_nodes, const std::vector<std::vector<int>>& adj_list, bool use_union_find) {
//     this->n = n_nodes;
//     this->use_union_find = use_union_find;
//     this->nodes.resize(n_nodes);
//     this->l_nodes.resize(n_nodes);
//     for (int i = 0; i < n_nodes; ++i) {
//         this->nodes[i].adj = adj_list[i]; // Accepts the empty or pre-filled adj
//         this->l_nodes[i].v = i;
//     }
//     this->init();
// }
// dndtree.h
inline DNDTree::DNDTree(int n_nodes, const std::vector<std::vector<int>>& adj_list, bool use_union_find) {
    this->n = n_nodes;
    this->use_union_find = use_union_find;
    this->nodes.resize(n_nodes);
    this->l_nodes.resize(n_nodes);
    
    for (int i = 0; i < n_nodes; ++i) {
        // Sanitize the input list: sort and remove duplicates/self-loops
        std::vector<int> sanitized = adj_list[i];
        
        // 1. Remove self-loops
        sanitized.erase(std::remove(sanitized.begin(), sanitized.end(), i), sanitized.end());
        
        // 2. Sort for deterministic find_replacement behavior
        std::sort(sanitized.begin(), sanitized.end());
        
        // 3. Remove duplicates
        sanitized.erase(std::unique(sanitized.begin(), sanitized.end()), sanitized.end());
        
        this->nodes[i].adj = std::move(sanitized);
        this->l_nodes[i].v = i;
    }
    
    // Perform standard tree initialization (DSU, component sizes, etc.)
    this->init();
}

inline void DNDTree::init()
{
    vector<pair<int, int>> s;
    used.resize(n, false);
    for (int vid = 0; vid < n; ++vid)
    {
        nodes[vid].flush();
        int len = nodes[vid].adj.size();
        s.push_back(make_pair(len, -vid));
    }
    sort(s.begin(), s.end());
    vector<int> q;

    if (use_union_find)
    {
        l_nodes.resize(n);

        for (int v = 0; v < n; ++v)
        {
            l_nodes[v].v = v;
            l_nodes[v].prev = NULL;
            l_nodes[v].next = NULL;
            nodes[v].f = v;
            nodes[v].l_start.next = &nodes[v].l_end;
            nodes[v].l_end.prev = &nodes[v].l_start;
            nodes[v].l_start.prev = NULL;
            nodes[v].l_end.next = NULL;
        }
    }

    for (int v = 0; v < n; ++v)
    {
        nodes[v].p = -1;
        nodes[v].sub_cnt = 1;
    }

    for (int i = n - 1; i >= 0; --i)
    {
        int f = -s[i].second;

        if (used[f])
            continue;
        q.clear();

        used[f] = true;
        q.push_back(f);

        if (use_union_find)
            nodes[f].insert_l_node(&l_nodes[f]);

        for (int s = 0; s < (int)q.size(); ++s)
        {
            int p = q[s];
            for (int j = 0; j < (int)nodes[p].adj.size(); ++j)
            {
                int v = nodes[p].adj[j];
                if (!used[v])
                {
                    used[v] = true;
                    q.push_back(v);
                    nodes[v].p = p;

                    if (use_union_find)
                    {
                        nodes[v].f = f;
                        nodes[f].insert_l_node(&l_nodes[v]);
                    }
                }
            }
        }

        for (int i = (int)q.size() - 1; i > 0; --i)
            nodes[nodes[q[i]].p].sub_cnt += nodes[q[i]].sub_cnt;

        int r = -1, ss = (int)q.size() / 2;
        for (int i = (int)q.size() - 1; i >= 0; --i)
            if (r == -1 && nodes[q[i]].sub_cnt > ss)
                r = q[i];
        if (r != f)
            reroot(r, f);
    }

    used.clear();
    used.resize(n, false);
}

inline bool DNDTree::query(int u, int v)
{
    if (u < 0 || u >= n || v < 0 || v >= n)
        return false;
    if (use_union_find)
        return get_f(u) == get_f(v);
    while (nodes[u].p != -1)
        u = nodes[u].p;
    while (nodes[v].p != -1)
        v = nodes[v].p;
    return u == v;
}

inline int DNDTree::get_f(int u)
{
    if (nodes[u].f != u)
    {
        int f = get_f(nodes[u].f);
        if (nodes[u].f != f)
        {
            nodes[u].f = f;
            l_nodes[u].isolate();
            nodes[f].insert_l_node(&l_nodes[u]);
        }
    }
    return nodes[u].f;
}

inline void DNDTree::union_f(int fu, int fv)
{ // fu->fv
    if (fu == fv)
        return;

    nodes[fu].f = fv;
    l_nodes[fu].isolate();
    nodes[fv].insert_l_node(&l_nodes[fu]);
}

inline void DNDTree::reroot(int u, int f)
{
    if (G_TRACE_ENABLED) {
        std::cout << "[CPP] reroot(" << u << ", " << f << ")" << std::endl;
        std::cout.flush();
    }

    int p, pp;
    for (p = nodes[u].p, nodes[u].p = -1; p != -1;)
        pp = nodes[p].p, nodes[p].p = u, u = p, p = pp;
    for (p = nodes[u].p; p != -1; u = p, p = nodes[p].p)
        nodes[u].sub_cnt -= nodes[p].sub_cnt, nodes[p].sub_cnt += nodes[u].sub_cnt;

    if (use_union_find && f >= 0)
    {
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   isolating(" << f << ")" << std::endl;
            std::cout.flush();
        }
        nodes[f].f = u;
        l_nodes[f].isolate();
        nodes[u].insert_l_node(&l_nodes[f]);
        
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   isolating(" << u << ")" << std::endl;
            std::cout.flush();
        }
        nodes[u].f = u;
        l_nodes[u].isolate();
        nodes[u].insert_l_node(&l_nodes[u]);
    }
}

inline int DNDTree::insert_edge_balanced(int u, int v)
{
    int fu, fv, p, pp, d;

    if (!use_union_find)
    {
        for (fu = u; nodes[fu].p != -1;)
        {
            fu = nodes[fu].p;
        }
        for (fv = v; nodes[fv].p != -1;)
        {
            fv = nodes[fv].p;
        }
    }
    else
    {
        fu = get_f(u);
        fv = get_f(v);
    }

    if (fu == fv)
    {
        bool reshape = false;
        for (d = 0, p = nodes[u].p, pp = nodes[v].p; d < MAXDEP; p = nodes[p].p, pp = nodes[pp].p, ++d)
            if (p == -1)
            {
                if (pp != -1 && nodes[pp].p != -1)
                {
                    reshape = true;
                    swap(u, v);
                    swap(p, pp);
                }
                break;
            }
            else if (pp == -1)
            {
                if (p != -1 && nodes[p].p != -1)
                    reshape = true;
                break;
            }
        if (reshape)
        {
            int dlt = 0;
            for (; p != -1; p = nodes[p].p)
                ++dlt;
            for (dlt = dlt / 2 - 1, p = u; dlt > 0; --dlt)
                p = nodes[p].p;

            for (pp = nodes[p].p; pp != -1; pp = nodes[pp].p)
                nodes[pp].sub_cnt -= nodes[p].sub_cnt;

            nodes[p].p = -1;
            reroot(u, -1);

            nodes[u].p = v;

            int s = (nodes[fu].sub_cnt + nodes[u].sub_cnt) / 2, r = -1;
            for (p = v; p != -1; p = nodes[p].p)
            {
                nodes[p].sub_cnt += nodes[u].sub_cnt;
                if (r == -1 && nodes[p].sub_cnt > s)
                    r = p;
            }
            if (r != fu) {
                reroot(r, fu);
                return 2;
            }
        }
        return 0;
    }
    if (nodes[fu].sub_cnt > nodes[fv].sub_cnt)
    {
        swap(u, v);
        swap(fu, fv);
    }

    for (p = nodes[u].p, nodes[u].p = v; p != -1;)
        pp = nodes[p].p, nodes[p].p = u, u = p, p = pp;

    int s = (nodes[fu].sub_cnt + nodes[fv].sub_cnt) / 2, r = -1;

    for (p = v; p != -1; p = nodes[p].p)
    {
        nodes[p].sub_cnt += nodes[fu].sub_cnt;
        if (r == -1 && nodes[p].sub_cnt > s)
            r = p;
    }

    for (p = nodes[u].p; p != v; u = p, p = nodes[p].p)
        nodes[u].sub_cnt -= nodes[p].sub_cnt, nodes[p].sub_cnt += nodes[u].sub_cnt;

    if (use_union_find)
        union_f(fu, fv);
    if (r != fv) {
        reroot(r, fv);
        return 3;
    }
    return 1;
}

inline int DNDTree::delete_edge_balanced(int u, int v)
{
    if (G_TRACE_ENABLED) {
        std::cout << "[CPP] delete_edge_balanced(" << u << ", " << v << ")" << std::endl;
        std::cout.flush();
    }

    if (nodes[u].p != v && nodes[v].p != u) {
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   early exit condition met... return 0" << std::endl;
            std::cout.flush();
        }
        return 0;
    }

    if (nodes[v].p == u) {
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]  swap condition met (nodes[v].p == u)..." << std::endl;
            std::cout.flush();
        }
        swap(u, v); // make u->v
    }


    int f;
    for (int w = v; w != -1; w = nodes[w].p)
        nodes[w].sub_cnt -= nodes[u].sub_cnt, f = w;
    nodes[u].p = -1;

    int ns, nl;
    bool needreroot;
    if (nodes[u].sub_cnt > nodes[f].sub_cnt)
    {
        ns = f;
        nl = u;
        needreroot = true;
    }
    else
    {
        ns = u;
        nl = f;
        needreroot = false;
    }

    if (use_union_find && needreroot)
    {
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   needreroot condition met..." << std::endl;
            std::cout.flush();
        }

        nodes[f].f = u;
        l_nodes[f].isolate();
        nodes[u].insert_l_node(&l_nodes[f]);

        nodes[u].f = u;
        l_nodes[u].isolate();
        nodes[u].insert_l_node(&l_nodes[u]);
    }

    if (find_replacement(ns, nl))
        return 1;

    if (use_union_find)
        remove_subtree_union_find(ns, nl, needreroot);

    return 2;
}

inline bool DNDTree::find_replacement(int u, int f)
{
    if (G_TRACE_ENABLED) {
        std::cout << "[CPP] find_replacement(" << u << ", " << f << ")" << std::endl;
        std::cout.flush();
    }

    q.clear();
    l.clear();
    q.push_back(u);
    l.push_back(u);
    used[u] = true;

    for (int i = 0; i < (int)q.size(); ++i)
    {
        int x = q[i], p, pp;
        nodes[x].flush();
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   scanning neighbors of (" << x << ")" << std::endl;
            std::cout << "[CPP]     adj for node " << u << ": ";
            for (int neighbor : nodes[u].adj) { 
                std::cout << neighbor << " "; 
            }
            std::cout << std::endl;
            std::cout.flush();
        }
        for (int j = 0; j < (int)nodes[x].adj.size(); ++j)
        {
            int y = nodes[x].adj[j];

            if (y == nodes[x].p)
                continue;
            if (nodes[y].p == x)
            {
                q.push_back(y);
                if (!used[y])
                {
                    used[y] = true;
                    l.push_back(y);
                }
                continue;
            }
            bool succ = true;
            for (int w = y; w != -1; w = nodes[w].p)
            {
                if (used[w])
                {
                    succ = false;
                    break;
                }
                used[w] = true;
                l.push_back(w);
            }
            if (!succ)
                continue;

            for (p = nodes[x].p, nodes[x].p = y; p != -1;)
                pp = nodes[p].p, nodes[p].p = x, x = p, p = pp;

            int s = (nodes[f].sub_cnt + nodes[u].sub_cnt) / 2, r = -1;
            for (p = y; p != -1; p = nodes[p].p)
            {
                nodes[p].sub_cnt += nodes[u].sub_cnt;
                if (r == -1 && nodes[p].sub_cnt > s)
                    r = p;
            }

            for (p = nodes[x].p; p != y; x = p, p = nodes[p].p)
                nodes[x].sub_cnt -= nodes[p].sub_cnt, nodes[p].sub_cnt += nodes[x].sub_cnt;
            for (int k = 0; k < (int)l.size(); ++k)
                used[l[k]] = false;

            if (G_TRACE_ENABLED) {
                std::cout << "[CPP]   replacement found (" << r << ")" << std::endl;
                std::cout.flush();
            }

            if (r != f)
                reroot(r, f);
            return true;
        }
    }
    for (int k = 0; k < (int)l.size(); ++k)
        used[l[k]] = false;

        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   replacement not found..." << std::endl;
            std::cout.flush();
        }

        return false;
}

inline void DNDTree::remove_subtree_union_find(int u, int v, bool needreroot)
{
    int fv = v;
    for (int i = 0; i < (int)q.size(); ++i)
    {
        int x = q[i];
        if (nodes[x].l_start.next != &nodes[x].l_end)
        {
            for (LinkNode *y = nodes[x].l_start.next; y != &nodes[x].l_end; y = y->next)
                nodes[y->v].f = fv;
            nodes[fv].insert_l_nodes(&nodes[x]);
        }
    }

    for (int i = 0; i < (int)q.size(); ++i)
    {
        int x = q[i];
        l_nodes[x].isolate();
        nodes[u].insert_l_node(&l_nodes[x]);
        nodes[x].f = u;
    }
}

inline int DNDTree::insert_edge(int u, int v)
{
    if (!insert_edge_in_graph(u, v))
        return -1;
    return insert_edge_balanced(u, v);
}

inline int DNDTree::delete_edge(int u, int v)
{
    if (G_TRACE_ENABLED) {
        std::cout << "[CPP] delete_edge(" << u << ", " << v << ")" << std::endl;
        std::cout.flush();
    }

    if (!delete_edge_in_graph(u, v))
        return -1;
    return delete_edge_balanced(u, v);
}

inline bool DNDTree::insert_edge_in_graph(int u, int v)
{
    if (u < 0 || u >= n || v < 0 || v >= n || u == v)
        return false;
    nodes[u].insert_adj(v);
    nodes[v].insert_adj(u);
    return true;
}

inline bool DNDTree::delete_edge_in_graph(int u, int v)
{
    if (G_TRACE_ENABLED) {
        std::cout << "[CPP] delete_edge_in_graph(" << u << ", " << v << ")" << std::endl;
        std::cout.flush();
    }

    if (u < 0 || u >= n || v < 0 || v >= n || u == v) {
        if (G_TRACE_ENABLED) {
            std::cout << "[CPP]   early exit condition met..." << std::endl;
            std::cout.flush();
        }
        return false;
    }

    if (G_TRACE_ENABLED) {
        std::cout << "[CPP]   deleting adj..." << std::endl;
        std::cout.flush();
    }

    nodes[u].delete_adj(v);
    nodes[v].delete_adj(u);
    return true;
}

#endif /* DNDTREE_H_ */
