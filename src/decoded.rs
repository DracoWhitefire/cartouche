/// A successfully decoded InfoFrame paired with any warnings produced during decoding.
///
/// `T` is the decoded frame type (e.g. [`AviInfoFrame`](crate::avi::AviInfoFrame));
/// `W` is the corresponding warning enum (e.g. [`AviWarning`](crate::warn::AviWarning)).
///
/// `Decoded<T, W>` is the `Ok` variant of
/// `Result<Decoded<T, W>, `[`DecodeError`](crate::error::DecodeError)`>`.
/// A non-empty warning list does not mean decoding failed — the frame is present
/// and usable. The caller decides how to act on the warnings.
///
/// # Accessing warnings
///
/// Use [`iter_warnings`](Decoded::iter_warnings) regardless of feature flags:
///
/// ```rust
/// # use cartouche::decoded::Decoded;
/// # use cartouche::warn::AviWarning;
/// # fn example(decoded: Decoded<(), AviWarning>) {
/// for warning in decoded.iter_warnings() {
///     eprintln!("warning: {warning:?}");
/// }
/// # }
/// ```
///
/// # Warning storage
///
/// The internal storage layout depends on the active feature flags:
///
/// - With `alloc` or `std`: warnings are stored in a `Vec<W>`, with no cap.
/// - Without either: warnings are stored in a fixed `[Option<W>; 8]` array.
///   Warnings beyond the 8-slot cap are silently dropped. In practice no
///   InfoFrame decode path produces more than a handful of warnings; this
///   limit is a safeguard, not an expected boundary.
pub struct Decoded<T, W> {
    /// The decoded frame.
    pub value: T,

    /// Warnings produced during decoding (alloc / std builds).
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub warnings: alloc::vec::Vec<W>,

    /// Warnings produced during decoding (bare no_std builds).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub warnings: [Option<W>; 8],

    /// Number of valid entries in `warnings` (bare no_std builds only).
    ///
    /// Warnings beyond index `num_warnings - 1` are `None` and must not be read.
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub num_warnings: usize,
}

impl<T, W> Decoded<T, W> {
    /// Construct a `Decoded` with no warnings.
    pub(crate) fn new(value: T) -> Self {
        Self {
            value,
            #[cfg(any(feature = "alloc", feature = "std"))]
            warnings: alloc::vec::Vec::new(),
            #[cfg(not(any(feature = "alloc", feature = "std")))]
            warnings: [const { None }; 8],
            #[cfg(not(any(feature = "alloc", feature = "std")))]
            num_warnings: 0,
        }
    }

    /// Append a warning.
    ///
    /// In bare `no_std` builds, warnings beyond the 8-slot cap are silently dropped.
    pub(crate) fn push_warning(&mut self, warning: W) {
        #[cfg(any(feature = "alloc", feature = "std"))]
        self.warnings.push(warning);

        #[cfg(not(any(feature = "alloc", feature = "std")))]
        if self.num_warnings < 8 {
            self.warnings[self.num_warnings] = Some(warning);
            self.num_warnings += 1;
        }
    }

    /// Convert this `Decoded<T, W>` into a `Decoded<U, V>` by mapping both the
    /// decoded value and each warning.
    ///
    /// Ownership of both fields is transferred; no cloning is required.
    pub(crate) fn wrap<U, V>(
        self,
        frame_fn: impl FnOnce(T) -> U,
        warn_fn: impl Fn(W) -> V,
    ) -> Decoded<U, V> {
        let mut out = Decoded::new(frame_fn(self.value));

        #[cfg(any(feature = "alloc", feature = "std"))]
        for w in self.warnings {
            out.push_warning(warn_fn(w));
        }

        #[cfg(not(any(feature = "alloc", feature = "std")))]
        {
            let n = self.num_warnings;
            for (i, opt) in self.warnings.into_iter().enumerate() {
                if i >= n {
                    break;
                }
                if let Some(w) = opt {
                    out.push_warning(warn_fn(w));
                }
            }
        }

        out
    }

    /// Iterate over all warnings produced during decoding.
    ///
    /// The iterator yields references to each warning in the order they were
    /// recorded. If no warnings were produced, the iterator is empty.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn iter_warnings(&self) -> impl Iterator<Item = &W> {
        self.warnings.iter()
    }

    /// Iterate over all warnings produced during decoding.
    ///
    /// The iterator yields references to each warning in the order they were
    /// recorded. If no warnings were produced, the iterator is empty.
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub fn iter_warnings(&self) -> impl Iterator<Item = &W> {
        self.warnings[..self.num_warnings]
            .iter()
            .filter_map(|w| w.as_ref())
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl<T: core::fmt::Debug, W: core::fmt::Debug> core::fmt::Debug for Decoded<T, W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Decoded")
            .field("value", &self.value)
            .field("warnings", &self.warnings)
            .finish()
    }
}

#[cfg(not(any(feature = "alloc", feature = "std")))]
impl<T: core::fmt::Debug, W: core::fmt::Debug> core::fmt::Debug for Decoded<T, W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Decoded")
            .field("value", &self.value)
            .field("warnings", &&self.warnings[..self.num_warnings])
            .finish()
    }
}
