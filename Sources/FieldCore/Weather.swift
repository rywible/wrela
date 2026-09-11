import Foundation
import simd

/// A periodic 64 km moist-air column model, sampled every two seconds. Horizontal
/// transport conserves water; saturation adjustment exchanges vapour/liquid and
/// latent heat. This is a 2D column approximation, not a 3D atmospheric solver.
public struct MoistWeather {
    public let size=64
    public private(set) var ticks=0
    /// Vapour and suspended liquid in g/kg, temperature in °C, updraft in m/s.
    public private(set) var cells:[SIMD4<Float>]
    public private(set) var evaporated:Double=0
    public private(set) var precipitated:Double=0
    private var nextCells=Array(repeating:SIMD4<Float>.zero,count:4096)
    private var heating:[Float]
    private var fluxVelocity:[SIMD2<Float>]
    public init(seed:UInt64) {
        var rng=SeededRandom(seed:seed &+ 7819)
        let phases=(0..<8).map{_ in rng.range(0,2 * .pi)}
        var heat=[Float](),moist=[Float](),stream=[Float]()
        for y in 0..<64 {for x in 0..<64 {
            let a=Float(x)*2 * .pi/64,b=Float(y)*2 * .pi/64
            heat.append(0.5*sin(a*2+b+phases[0])+0.3*cos(a-b*3+phases[1])+0.2*sin(a*4+b*2+phases[2]))
            moist.append(0.6*sin(a+b*2+phases[3])+0.4*cos(a*3-b+phases[4]))
            stream.append(30_000*(sin(a+b*2+phases[5])+0.4*cos(a*3-b+phases[6])))
        }}
        heating=heat;cells=[];fluxVelocity=[]
        for y in 0..<64 {for x in 0..<64 {
            let i=y*64+x,t:Float=12+heat[i]*0.8
            cells.append(SIMD4(Self.saturation(t)*(1+moist[i]*0.035),0.2+moist[i]*0.12,t,heat[i]*1.4))
            // Discrete curl at cell faces: the transport velocity is divergence free.
            fluxVelocity.append(SIMD2(12+(stream[((y+1)%64)*64+(x+1)%64]-stream[y*64+(x+1)%64])/1000,
                                      4-(stream[((y+1)%64)*64+(x+1)%64]-stream[((y+1)%64)*64+x])/1000))
        }}
    }
    public static func saturation(_ temperature:Float)->Float {saturationSample(temperature).value}
    /// The clamped function has zero derivative outside the open temperature
    /// interval, including its nondifferentiable endpoints (explicit convention).
    public static func saturationSample(_ temperature:Float)->(value:Float,slope:Float) {
        let t=min(30,max(-25,temperature)),den=t+243.5,e=6.112*exp(17.67*t/den),pressure=800-e
        let q=622*e/pressure
        let slope=temperature > -25 && temperature < 30 ? 622*800*e/(pressure*pressure)*(17.67*243.5/(den*den)):0
        return (q,slope)
    }
    private func index(_ x:Int,_ y:Int)->Int {((y+64)%64)*64+(x+64)%64}
    public mutating func advance(to seconds:Float,sun:Float) {
        let target=max(0,Int(seconds/2))
        while ticks<target {step(sun:sun)}
    }
    public mutating func step(sun:Float) {
        let dt:Float=2
        for y in 0..<64 {for x in 0..<64 {
            let i=index(x,y),left=index(x-1,y),right=index(x+1,y),down=index(x,y-1),up=index(x,y+1)
            let vx=fluxVelocity[i].x,vl=fluxVelocity[left].x,vy=fluxVelocity[i].y,vd=fluxVelocity[down].y
            let outgoingX=(vx>=0 ? cells[i]:cells[right])*vx,incomingX=(vl>=0 ? cells[left]:cells[i])*vl
            let outgoingY=(vy>=0 ? cells[i]:cells[up])*vy,incomingY=(vd>=0 ? cells[down]:cells[i])*vd
            var c=cells[i]-(outgoingX-incomingX+outgoingY-incomingY)*(dt/1000)
            c.w=clamp(c.w+((c.z-12)*0.08-c.w*0.04)*dt,-6,8)
            c.z+=((12+heating[i]*max(sun,0)*1.8-c.z)/240-c.w*0.0098)*dt
            let (q,slope)=Self.saturationSample(c.z)
            var transfer=(c.x-q)/(1+2.49*slope)*(1-exp(-dt/12))
            transfer=clamp(transfer,-c.y,c.x)
            c.x-=transfer;c.y+=transfer;c.z+=2.49*transfer
            let source:Float=0.0005*max(sun,0)*dt,rain=max(0,c.y-1.2)*0.001*dt
            c.x+=source;c.y-=rain;evaporated+=Double(source);precipitated+=Double(rain)
            nextCells[i]=c
        }}
        swap(&cells,&nextCells)
        ticks+=1
    }
    public var totalWater:Double {cells.reduce(0){$0+Double($1.x)+Double($1.y)}}
    public var gpuCells:[SIMD4<Float>] {cells.map{SIMD4($0.y,$0.z,$0.w,$0.x)}}
}
