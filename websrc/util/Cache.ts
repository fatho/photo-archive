export class Lru<K, V> {
    private nodeMap: Map<K, CacheNode<K, V>>;
    private newest: CacheNode<K, V> | null;
    private oldest: CacheNode<K, V> | null;

    constructor() {
        this.nodeMap = new Map();
        this.newest = null;
        this.oldest = null;
    }

    evict(count: number): Evicted<K, V>[] {
        let evicted: Evicted<K, V>[] = [];


        while(this.oldest != null && count > 0) {
            let key = this.oldest.key;
            let value = this.oldest.value;
            this.oldest = this.oldest.newer;
            if (this.oldest != null) {
                this.oldest.older = null;
            }
            this.nodeMap.delete(key);
            evicted.push(new Evicted(key, value));
            count -= 1;
        }
        this.verify();

        return evicted;
    }

    insert(key: K, value: V) {
        let node = new CacheNode(key, value);
        if (this.newest == null) {
            // cache is empty
            this.nodeMap.set(key, node);
            this.newest = node;
            this.oldest = node;
        } else {
            node.older = this.newest;
            this.newest.newer = node;
            this.newest = node;
            this.nodeMap.set(key, node);
        }
        this.verify();
    }

    get(key: K): V | undefined {
        let node = this.nodeMap.get(key);
        if (node == undefined) {
            return undefined;
        } else {
            if(node == this.newest) {
                // nothing to do
            } else if (node == this.oldest) {
                this.oldest = node.newer;
                (node.newer as CacheNode<K,V>).older = null;

                node.older = this.newest;
                node.newer = null;
                (this.newest as CacheNode<K, V>).newer = node;
                this.newest = node;
            } else {
                (node.older as CacheNode<K,V>).newer = node.newer;
                (node.newer as CacheNode<K,V>).older = node.older;

                node.older = this.newest;
                node.newer = null;
                (this.newest as CacheNode<K, V>).newer = node;
                this.newest = node;
            }
            // if (node.newer != null) {
            //     // The node was not the least recently used
            //     node.newer.older = node.older;
            //     if (node.older == null) {
            //         // the node was the oldest one, so there is a new oldest one now
            //         this.oldest = node.newer;
            //     } else {
            //         node.older.newer = node.newer;
            //     }
            //     // The node is becoming the newest
            //     node.newer = null;
            //     // we know that newest is not null here because the cache has at least one entry
            //     node.older = this.newest;
            //     (this.newest as CacheNode<K, V>).newer = node;
            // }
            this.verify();
            return node.value;
        }
    }

    delete(key: K) {
        let node = this.nodeMap.get(key);
        if(node != undefined) {
            this.nodeMap.delete(key);
            if(node.newer == null) {
                this.newest = node.older;
            } else {
                node.newer.older = node.older;
            }
            if(node.older == null) {
                this.oldest = node.newer;
            } else {
                node.older.newer = node.newer;
            }
        }
        this.verify();
    }

    private verify() {
        let current = this.newest;
        let count = 0;
        while(current != null) {
            count += 1;
            if(current.older == null) {
                if(current != this.oldest) {
                    throw new Error('oldest not oldest');
                }
            }
            current = current.older;
        }
        if(count != this.nodeMap.size) {
            throw new Error('wrong size');
        }
    }

    get size(): number {
        return this.nodeMap.size;
    }
}

class CacheNode<K, V> {
    newer: CacheNode<K, V> | null = null;
    older: CacheNode<K, V> | null = null;
    readonly key: K;
    readonly value: V;

    constructor(key: K, value: V) {
        this.key = key;
        this.value = value;
    }
}

class Evicted<K, V> {
    readonly key: K;
    readonly value: V;

    constructor(key: K, value: V) {
        this.key = key;
        this.value = value;
    }
}