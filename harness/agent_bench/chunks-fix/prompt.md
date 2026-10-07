Users report that `chunks` in `batch.lot` loses items at the end of the list. Fix it.

Also, a size below 1 must now be an error: `chunks` returns `[[int]] ! SizeErr`, failing with `BadSize(size)` (the type is already declared), and `batch_sums` returns `[int] ! SizeErr`, passing the error on. Update the tests to the new signatures.
