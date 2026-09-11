import Foundation

/// Convex-hull certificates for a polynomial represented in Bernstein form.
/// A same-sign coefficient hull excludes roots over the entire interval.
public struct BernsteinPolynomial {
    public let coefficients:[Float]
    public init(_ coefficients:[Float]) {precondition(!coefficients.isEmpty);self.coefficients=coefficients}
    public var range: ClosedRange<Float> {coefficients.min()!...coefficients.max()!}
    public var excludesZero:Bool {range.lowerBound>0 || range.upperBound<0}
    public func split()->(BernsteinPolynomial,BernsteinPolynomial) {
        var row=coefficients,left=[row.first!],right=[row.last!]
        while row.count>1 {row=zip(row,row.dropFirst()).map{($0+$1)*0.5};left.append(row.first!);right.append(row.last!)}
        return (BernsteinPolynomial(left),BernsteinPolynomial(right.reversed()))
    }
}
