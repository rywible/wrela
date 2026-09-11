import Foundation
import simd

/// A small horizontal flow simulation in metres/seconds. Advection transports
/// gusts through space; a pressure solve removes divergence. Vegetation follows
/// damped springs driven by this flow, rather than a periodic animation clock.
public struct WindSimulation: Codable {
    enum CodingKeys:String,CodingKey {case flow,response,previousResponse,velocity,rng,forcing,elapsed,ticks}
    public let size = 32
    public let cellMetres: Float = 10
    public var flow: [SIMD2<Float>]
    public var response: [SIMD2<Float>]
    private var previousResponse:[SIMD2<Float>]
    private var velocity: [SIMD2<Float>]
    // Transient scratch is deliberately absent from saved simulation state.
    private var advected=Array(repeating:SIMD2<Float>.zero,count:1024)
    private var pressure=Array(repeating:Float(0),count:1024)
    private var nextPressure=Array(repeating:Float(0),count:1024)
    private var divergence=Array(repeating:Float(0),count:1024)
    private var rng = SeededRandom(seed: 46291)
    private var forcing = SIMD2<Float>(3,1)
    private var elapsed: Float = 0
    public private(set) var ticks = 0
    public init() {
        flow = Array(repeating: SIMD2(3,1), count: 1024)
        response = Array(repeating: .zero, count: 1024)
        velocity = response;previousResponse=response
    }
    private func index(_ x:Int,_ y:Int)->Int { (y & 31)*32+(x & 31) }
    private func sample(_ a:[SIMD2<Float>],_ p:SIMD2<Float>)->SIMD2<Float> {
        let x=Int(floor(p.x)),y=Int(floor(p.y)),f=p-SIMD2(Float(x),Float(y))
        return simd_mix(simd_mix(a[index(x,y)],a[index(x+1,y)],SIMD2(repeating:f.x)),simd_mix(a[index(x,y+1)],a[index(x+1,y+1)],SIMD2(repeating:f.x)),SIMD2(repeating:f.y))
    }
    public mutating func step(_ dt:Float) {
        elapsed += dt
        guard elapsed >= 1/15 else {return}
        let h:Float=1/15;elapsed -= h;ticks += 1
        for i in response.indices {previousResponse[i]=response[i]}
        forcing += (SIMD2(3,1)-forcing)*h*0.3 + SIMD2(rng.range(-1,1),rng.range(-1,1))*sqrt(h)*0.8
        for y in 0..<size { for x in 0..<size {
            let i=index(x,y),p=SIMD2(Float(x),Float(y))
            advected[i]=sample(flow,p-flow[i]*(h/cellMetres))
            advected[i] += (forcing-advected[i])*h*0.15
        }}
        swap(&flow,&advected)
        // Seeded, localized vortices enter the flow and are carried downstream.
        if ticks % 8 == 0 {
            let center=SIMD2(rng.range(0,32),rng.range(0,32)),strength=rng.range(-3,3)
            for y in 0..<size {for x in 0..<size {
                let d=SIMD2(Float(x),Float(y))-center
                flow[index(x,y)] += SIMD2(-d.y,d.x)*exp(-simd_length_squared(d)/12)*strength*0.12
            }}
        }
        projectFlow()
        for i in flow.indices {
            let target=flow[i]*0.035
            velocity[i] += ((target-response[i])*18-velocity[i]*5)*h
            response[i] += velocity[i]*h
            response[i]=simd_clamp(response[i],SIMD2(repeating:-0.35),SIMD2(repeating:0.35))
        }
    }
    /// Compatible projection on the existing collocated periodic grid. Centered
    /// divergence composed with centered gradient is a stride-two Laplacian,
    /// not the nearest-neighbor operator. Weighted Jacobi damps its high modes.
    /// Keeping the grid locations also preserves the saved flow/response ABI.
    mutating func projectFlow(iterations:Int=12) {
        for y in 0..<size {for x in 0..<size {
            let i=index(x,y)
            divergence[i]=(flow[index(x+1,y)].x-flow[index(x-1,y)].x+flow[index(x,y+1)].y-flow[index(x,y-1)].y)*0.5
            pressure[i]=0
        }}
        for _ in 0..<iterations {
            for y in 0..<size {for x in 0..<size {
                let i=index(x,y)
                let candidate=(pressure[index(x-2,y)]+pressure[index(x+2,y)]+pressure[index(x,y-2)]+pressure[index(x,y+2)]-4*divergence[i])*0.25
                nextPressure[i]=pressure[i]+0.8*(candidate-pressure[i])
            }}
            swap(&pressure,&nextPressure)
        }
        for y in 0..<size {for x in 0..<size {
            flow[index(x,y)] -= SIMD2(pressure[index(x+1,y)]-pressure[index(x-1,y)],pressure[index(x,y+1)]-pressure[index(x,y-1)])*0.5
        }}
    }
    public var validSnapshot:Bool {
        [response,previousResponse].allSatisfy {a in a.allSatisfy {abs($0.x)<=0.35 && abs($0.y)<=0.35}} && [flow,response,previousResponse,velocity].allSatisfy {a in a.count==1024 && a.allSatisfy{$0.x.isFinite && $0.y.isFinite && abs($0.x)<1000 && abs($0.y)<1000}} && forcing.x.isFinite && forcing.y.isFinite && abs(forcing.x)<1000 && abs(forcing.y)<1000 && elapsed.isFinite && (0...0.1).contains(elapsed) && ticks>=0
    }
    public func writeGPUCells(to output:UnsafeMutableBufferPointer<SIMD4<Float>>) {
        precondition(output.count==flow.count)
        let alpha=clamp(elapsed*15,0,1)
        for i in flow.indices {
            let r=simd_mix(previousResponse[i],response[i],SIMD2(repeating:alpha))
            output[i]=SIMD4(flow[i].x,flow[i].y,r.x,r.y)
        }
    }
    public var gpuCells:[SIMD4<Float>] { let alpha=clamp(elapsed*15,0,1);return flow.indices.map{let r=simd_mix(previousResponse[$0],response[$0],SIMD2(repeating:alpha));return SIMD4(flow[$0].x,flow[$0].y,r.x,r.y)} }
}
