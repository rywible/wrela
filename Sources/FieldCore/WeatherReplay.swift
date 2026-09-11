import Foundation

/// Preserves the existing fixed-forcing editing convention. Checkpoints include
/// every simulation buffer and are keyed by exact seed, forcing and integer tick.
/// A bounded LRU prevents repeated preset edits from retaining unbounded history.
public struct WeatherReplay {
    private struct Key:Hashable {var seed:UInt64;var sun:UInt32;var tick:Int}
    private var checkpoints:[Key:MoistWeather]=[:]
    private var order:[Key]=[]
    private var current:MoistWeather?
    private var currentSeed:UInt64=0
    private var currentSun:UInt32=0
    public private(set) var replayedSteps=0
    public init() {}
    public mutating func advance(to seconds:Float,seed:UInt64,sun:Float)->MoistWeather {
        let target=max(0,Int(seconds/2)),forcing=sun.bitPattern
        var state:MoistWeather
        if let current,currentSeed==seed,currentSun==forcing,current.ticks<=target {state=current}
        else {state=MoistWeather(seed:seed)}
        if let key=order.filter({$0.seed==seed && $0.sun==forcing && $0.tick<=target && $0.tick>state.ticks}).max(by:{$0.tick<$1.tick}),let saved=checkpoints[key] {
            state=saved
            order.removeAll {$0==key};order.append(key)
        }
        // Release the previous reference before stepping to allow scratch reuse.
        current=nil;replayedSteps=0
        while state.ticks<target {
            state.step(sun:sun);replayedSteps+=1
            if state.ticks%30==0 {
                let key=Key(seed:seed,sun:forcing,tick:state.ticks)
                checkpoints[key]=state;order.removeAll {$0==key};order.append(key)
                if order.count>64 {checkpoints.removeValue(forKey:order.removeFirst())}
            }
        }
        current=state;currentSeed=seed;currentSun=forcing
        return state
    }
}
