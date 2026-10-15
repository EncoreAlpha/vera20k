//! Order-preserving work over rayon's persistent worker pool. Each result
//! depends on its item alone, so the output never depends on the thread
//! count or on scheduling. Work run [`alongside`] the caller and the caller
//! must only read what they share: the borrow checker rules out a shared
//! `&mut`, not interior mutability such as the shared cell dummy's
//! coordinate. The pool's threads persist, so their thread-local scratch (the
//! path search workspaces) stays allocated between frames.

use rayon::prelude::*;

/// `work` over every item, results in item order. Workers steal items from
/// each other, so slower cores (efficiency cores, a busy host) take fewer.
/// A worker's panic resumes on the caller.
pub(crate) fn map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    // A shared reference to a `Sync` closure is `Send`.
    items.par_iter().map(&work).collect()
}

/// The indices below `len` that `keep` selects, in ascending order.
pub(crate) fn select_indices(len: usize, keep: impl Fn(usize) -> bool + Sync) -> Vec<usize> {
    (0..len)
        .into_par_iter()
        .filter(|&index| keep(index))
        .collect()
}

/// Whether `test` holds for any index below `len`.
pub(crate) fn any_index(len: usize, test: impl Fn(usize) -> bool + Sync) -> bool {
    (0..len).into_par_iter().any(&test)
}

/// Run `pooled` on the worker pool while `here` runs on this thread, and
/// return once both are done. Only `pooled` crosses threads, so `here` may
/// hold this thread's `&mut` borrows and read its thread-locals. A panic in
/// either resumes on the caller.
pub(crate) fn alongside(pooled: impl FnOnce() + Send, here: impl FnOnce()) {
    rayon::in_place_scope(|scope| {
        scope.spawn(move |_| pooled());
        here();
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn results_keep_item_order() {
        let items: Vec<u64> = (0..1000).collect();
        let squares = super::map(&items, |item| item * item);
        assert_eq!(
            squares,
            items.iter().map(|item| item * item).collect::<Vec<_>>()
        );
        assert!(super::map(&[] as &[u64], |item| *item).is_empty());
        assert_eq!(
            super::select_indices(1000, |index| index % 7 == 3),
            (0..1000).filter(|index| index % 7 == 3).collect::<Vec<_>>()
        );
        assert!(super::any_index(1000, |index| index == 999));
        assert!(!super::any_index(1000, |index| index >= 1000));
    }

    #[test]
    fn alongside_runs_here_on_the_caller_and_waits_for_the_pooled_work() {
        let items: Vec<u64> = (0..1000).collect();
        let mut pooled_sum = 0;
        let mut here_thread = None;
        super::alongside(
            || pooled_sum = super::map(&items, |item| item * 3).iter().sum(),
            || here_thread = Some(std::thread::current().id()),
        );
        assert_eq!(
            (here_thread, pooled_sum),
            (Some(std::thread::current().id()), 3 * 499_500)
        );
        // On a one-thread pool's own worker, the caller runs the pooled work.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("a worker pool");
        let mut ran = false;
        pool.install(|| super::alongside(|| ran = true, || ()));
        assert!(ran);
    }
}
