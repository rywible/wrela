import Foundation

/// Static broad phase over conservative world bounds, independent of the view.
/// IDs remain in source order so sequential contact resolution stays stable.
public struct SpatialBoundsGrid {
    private var bins:[SIMD2<Int>:[Int]]=[:]
    private let cellSize:Float
    public init(bounds:[Bounds],cellSize:Float=16) {
        precondition(cellSize>0 && cellSize.isFinite);self.cellSize=cellSize
        for (id,b) in bounds.enumerated() {
            let lo=cell(b.min),hi=cell(b.max)
            for z in lo.y...hi.y {for x in lo.x...hi.x {bins[SIMD2(x,z),default:[]].append(id)}}
        }
    }
    private func cell(_ p:V3)->SIMD2<Int> {SIMD2(Int((p.x/cellSize).rounded(.down)),Int((p.z/cellSize).rounded(.down)))}
    public func candidates(at p:V3)->[Int] {bins[cell(p)] ?? []}
}
