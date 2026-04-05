/// An HDR Static Metadata InfoFrame.
///
/// Carries the EOTF and HDR static metadata as defined in CTA-861-G, including
/// display mastering luminance, primaries, white point, MaxCLL, and MaxFALL.
/// Required for HDR10 and HLG content.
///
/// Fields and encode/decode support are added in a subsequent implementation
/// phase.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HdrStaticInfoFrame {}
