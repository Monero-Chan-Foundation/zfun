//! Advanced optimization utilities for performance-critical operations
//!
//! This module contains unsafe and SIMD-optimized operations for maximum performance.
//! All unsafe code is carefully documented and bounded.

use crate::shard_file::Node;

/// Fast node copy using unsafe pointer operations
///
/// SAFETY: This is safe when:
/// - src and dst are valid Node references
/// - They don't overlap (use ptr::copy_nonoverlapping)
#[inline(always)]
pub unsafe fn fast_node_copy(src: &Node, dst: &mut Node) {
    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 32);
    }
}

/// Batch copy nodes with SIMD hints
///
/// SAFETY: src and dst must not overlap and must have the same length
#[inline]
pub unsafe fn batch_copy_nodes(src: &[Node], dst: &mut [Node]) {
    unsafe {
        debug_assert_eq!(src.len(), dst.len());
        std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), src.len());
    }
}

/// Thread-local buffer pool for reducing allocations
pub struct NodeBufferPool {
    buffers: Vec<Vec<Node>>,
    in_use: usize,
}

impl NodeBufferPool {
    /// Create a new buffer pool with capacity for N buffers
    pub fn new(capacity: usize) -> Self {
        Self {
            buffers: Vec::with_capacity(capacity),
            in_use: 0,
        }
    }

    /// Get a buffer with at least min_capacity, reusing if possible
    pub fn get_buffer(&mut self, min_capacity: usize) -> Vec<Node> {
        if let Some(mut buffer) = self.buffers.pop() {
            buffer.clear();
            if buffer.capacity() < min_capacity {
                buffer.reserve(min_capacity - buffer.capacity());
            }
            self.in_use += 1;
            buffer
        } else {
            self.in_use += 1;
            Vec::with_capacity(min_capacity)
        }
    }

    /// Return a buffer to the pool for reuse
    pub fn return_buffer(&mut self, buffer: Vec<Node>) {
        if self.in_use > 0 {
            self.in_use -= 1;
        }
        // Only keep buffers if we haven't exceeded pool size
        if self.buffers.len() < self.buffers.capacity() {
            self.buffers.push(buffer);
        }
    }
}

thread_local! {
    /// Thread-local buffer pool for tree building
    static TREE_BUFFER_POOL: std::cell::RefCell<NodeBufferPool> =
        std::cell::RefCell::new(NodeBufferPool::new(8));
}

/// Get a buffer from the thread-local pool
#[inline]
pub fn get_pooled_buffer(min_capacity: usize) -> Vec<Node> {
    TREE_BUFFER_POOL.with(|pool| pool.borrow_mut().get_buffer(min_capacity))
}

/// Return a buffer to the thread-local pool
#[inline]
pub fn return_pooled_buffer(buffer: Vec<Node>) {
    TREE_BUFFER_POOL.with(|pool| pool.borrow_mut().return_buffer(buffer));
}

/// Prefetch memory for better cache performance
///
/// SAFETY: ptr must be valid and aligned
#[inline(always)]
pub unsafe fn prefetch_read<T>(ptr: *const T) {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        {
            std::arch::x86_64::_mm_prefetch::<3>(ptr as *const i8);
        }
        #[cfg(target_arch = "aarch64")]
        {
            std::arch::aarch64::__pld(ptr as *const u8);
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            // No-op on other architectures
            let _ = ptr;
        }
    }
}

/// Zero-copy slice transmutation for aligned data
///
/// SAFETY: Only safe when T and U have the same size and alignment
#[inline]
pub unsafe fn transmute_slice<T, U>(slice: &[T]) -> &[U] {
    unsafe {
        debug_assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<U>());
        debug_assert_eq!(std::mem::align_of::<T>(), std::mem::align_of::<U>());
        std::slice::from_raw_parts(slice.as_ptr() as *const U, slice.len())
    }
}

/// Fast parallel fill with a value
#[inline]
pub fn parallel_fill_nodes(dst: &mut [Node], value: &Node) {
    use rayon::prelude::*;

    // Only parallelize if beneficial (>1000 nodes)
    if dst.len() > 1000 {
        dst.par_chunks_mut(256).for_each(|chunk| {
            chunk.fill(*value);
        });
    } else {
        dst.fill(*value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool() {
        let mut pool = NodeBufferPool::new(4);

        let buf1 = pool.get_buffer(100);
        assert_eq!(buf1.capacity(), 100);

        pool.return_buffer(buf1);

        let buf2 = pool.get_buffer(50);
        assert!(buf2.capacity() >= 50);

        pool.return_buffer(buf2);
    }

    #[test]
    fn test_fast_node_copy() {
        let src = [42u8; 32];
        let mut dst = [0u8; 32];

        unsafe {
            fast_node_copy(&src, &mut dst);
        }

        assert_eq!(src, dst);
    }

    #[test]
    fn test_parallel_fill() {
        let value = [123u8; 32];
        let mut nodes = vec![[0u8; 32]; 2000];

        parallel_fill_nodes(&mut nodes, &value);

        assert!(nodes.iter().all(|n| n == &value));
    }
}
