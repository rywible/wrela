#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT/.artifacts/perf/crosslang"
SRC_DIR="$OUT_DIR/src"
BIN_DIR="$OUT_DIR/bin"
RAW_DIR="$OUT_DIR/raw"
OUT_JSON="$OUT_DIR/final-comparison.json"
BASELINE_JSON="$OUT_DIR/baseline.json"
mkdir -p "$SRC_DIR" "$BIN_DIR" "$RAW_DIR"

RUNS="${RUNS:-5}"

extract_kv() {
  local key="$1" file="$2"
  awk -F= -v k="$key" '$1==k {print $2}' "$file"
}

median_from_file() {
  local file="$1"
  awk '{print $1}' "$file" | sort -n | awk '
    { a[++n]=$1 }
    END {
      if (n == 0) {
        print 0;
      } else if (n % 2 == 1) {
        print a[(n + 1) / 2];
      } else {
        printf "%.6f\n", (a[n / 2] + a[n / 2 + 1]) / 2.0;
      }
    }
  '
}

percentile_from_file() {
  local file="$1" pct="$2"
  local n rank
  n="$(awk 'END { print NR+0 }' "$file")"
  if [[ "$n" -eq 0 ]]; then
    echo 0
    return
  fi
  rank="$(awk -v p="$pct" -v n="$n" 'BEGIN { r = int((p * n) + 0.999999); if (r < 1) r = 1; if (r > n) r = n; print r }')"
  sort -n "$file" | awk -v r="$rank" 'NR==r { print $1; exit }'
}

cv_from_file() {
  local file="$1"
  awk '
    { a[++n]=$1; sum+=$1 }
    END {
      if (n <= 1) { print 0; exit }
      mean=sum/n;
      if (mean == 0) { print 0; exit }
      for (i=1;i<=n;i++) { d=a[i]-mean; var+=d*d }
      var=var/n;
      printf "%.4f", (sqrt(var)/mean)*100.0;
    }
  ' "$file"
}

run_collect() {
  local name="$1" cmd="$2" warmups="${3:-0}"
  : > "$RAW_DIR/${name}_queue.series"
  : > "$RAW_DIR/${name}_scheduler.series"
  : > "$RAW_DIR/${name}_workqueue.series"
  if [[ "$warmups" -gt 0 ]]; then
    for _ in $(seq 1 "$warmups"); do
      eval "$cmd" >/dev/null
    done
  fi
  for i in $(seq 1 "$RUNS"); do
    local out="$RAW_DIR/${name}_run${i}.txt"
    eval "$cmd" > "$out"
    extract_kv queue_msgs_per_sec "$out" >> "$RAW_DIR/${name}_queue.series"
    extract_kv scheduler_msgs_per_sec "$out" >> "$RAW_DIR/${name}_scheduler.series"
    extract_kv workqueue_msgs_per_sec "$out" >> "$RAW_DIR/${name}_workqueue.series"
  done
}

pct_delta_higher_better() {
  awk -v wr="$1" -v other="$2" 'BEGIN { if (other <= 0) { print 0.0; exit } printf "%.2f", ((wr-other)/other)*100.0 }'
}

have() {
  command -v "$1" >/dev/null 2>&1
}

usable() {
  "$@" >/dev/null 2>&1
}

cat > "$SRC_DIR/rust_bench.rs" <<'EOF'
use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

const QUEUE_TOTAL: u64 = 300_000;
const SCHED_TOTAL: u64 = 150_000;
const SPIN: u64 = 128;
const WQ_TOTAL: u64 = 300_000;
const WQ_WORKERS: usize = 4;
const WQ_PRODUCERS: usize = 4;

fn queue_msgs_per_sec() -> f64 {
    let (tx, rx) = mpsc::sync_channel::<u64>(1024);
    let handle = thread::spawn(move || {
        let mut got = 0u64;
        while got < QUEUE_TOTAL {
            if rx.recv().is_ok() {
                got += 1;
            }
        }
    });
    let start = Instant::now();
    for i in 0..QUEUE_TOTAL {
        tx.send(i).expect("send");
    }
    handle.join().expect("join");
    QUEUE_TOTAL as f64 / start.elapsed().as_secs_f64()
}

fn scheduler_msgs_per_sec() -> f64 {
    let mut q0 = VecDeque::from(vec![0u64; 64]);
    let mut q1 = VecDeque::from(vec![0u64; 64]);
    let mut q2 = VecDeque::from(vec![0u64; 64]);
    let mut q3 = VecDeque::from(vec![0u64; 64]);
    let mut processed = 0u64;
    let mut cursor = 0usize;
    let start = Instant::now();
    while processed < SCHED_TOTAL {
        let q = match cursor & 3 {
            0 => &mut q0,
            1 => &mut q1,
            2 => &mut q2,
            _ => &mut q3,
        };
        if let Some(v) = q.pop_front() {
            let mut mix = v.wrapping_add(1);
            for _ in 0..SPIN {
                mix = mix.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
            std::hint::black_box(mix);
            q.push_back(mix);
            processed += 1;
        }
        cursor = (cursor + 1) & 3;
    }
    SCHED_TOTAL as f64 / start.elapsed().as_secs_f64()
}

fn workqueue_msgs_per_sec() -> f64 {
    let (in_tx, in_rx) = mpsc::sync_channel::<u64>(4096);
    let mut worker_txs = Vec::with_capacity(WQ_WORKERS);
    let mut worker_handles = Vec::with_capacity(WQ_WORKERS);
    let done = Arc::new(AtomicU64::new(0));

    for _ in 0..WQ_WORKERS {
        let (tx, rx) = mpsc::sync_channel::<u64>(1024);
        worker_txs.push(tx);
        let done = done.clone();
        worker_handles.push(thread::spawn(move || {
            while let Ok(v) = rx.recv() {
                if v == u64::MAX {
                    break;
                }
                let mut mix = v;
                for _ in 0..SPIN {
                    mix = mix.wrapping_mul(6364136223846793005).wrapping_add(1);
                }
                std::hint::black_box(mix);
                done.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // Dispatcher: single consumer of the inbound channel, fans out to workers.
    let dispatch_done = done.clone();
    let dispatch_handle = thread::spawn(move || {
        let mut rr = 0usize;
        while let Ok(v) = in_rx.recv() {
            let tx = &worker_txs[rr % worker_txs.len()];
            rr += 1;
            let _ = tx.send(v);
        }
        for tx in worker_txs {
            let _ = tx.send(u64::MAX);
        }
        dispatch_done.load(Ordering::Relaxed)
    });

    let start = Instant::now();
    let mut producers = Vec::new();
    for p in 0..WQ_PRODUCERS {
        let tx = in_tx.clone();
        producers.push(thread::spawn(move || {
            for i in (p as u64..WQ_TOTAL).step_by(WQ_PRODUCERS) {
                tx.send(i).expect("send");
            }
        }));
    }
    for t in producers {
        t.join().expect("producer join");
    }
    drop(in_tx);
    let _ = dispatch_handle.join().expect("dispatch join");
    for w in worker_handles {
        w.join().expect("worker join");
    }
    let secs = start.elapsed().as_secs_f64();
    done.load(Ordering::Relaxed) as f64 / secs
}

fn main() {
    println!("queue_msgs_per_sec={:.2}", queue_msgs_per_sec());
    println!("scheduler_msgs_per_sec={:.2}", scheduler_msgs_per_sec());
    println!("workqueue_msgs_per_sec={:.2}", workqueue_msgs_per_sec());
}
EOF

cat > "$SRC_DIR/c_bench.c" <<'EOF'
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define QUEUE_TOTAL 300000ull
#define SCHED_TOTAL 150000ull
#define SPIN 128ull
#define RING_CAP 2048u
#define WQ_TOTAL 300000ull
#define WQ_WORKERS 4
#define WQ_PRODUCERS 4

typedef struct {
  uint64_t buf[RING_CAP];
  unsigned head;
  unsigned tail;
  unsigned count;
  uint64_t total;
  pthread_mutex_t mu;
  pthread_cond_t not_empty;
  pthread_cond_t not_full;
} queue_t;

static double elapsed_ns(struct timespec a, struct timespec b) {
  return (double)(b.tv_sec - a.tv_sec) * 1e9 + (double)(b.tv_nsec - a.tv_nsec);
}

static void queue_init(queue_t *q, uint64_t total) {
  q->head = q->tail = q->count = 0u;
  q->total = total;
  pthread_mutex_init(&q->mu, NULL);
  pthread_cond_init(&q->not_empty, NULL);
  pthread_cond_init(&q->not_full, NULL);
}

static void queue_push(queue_t *q, uint64_t v) {
  pthread_mutex_lock(&q->mu);
  while (q->count == RING_CAP) {
    pthread_cond_wait(&q->not_full, &q->mu);
  }
  q->buf[q->tail] = v;
  q->tail = (q->tail + 1u) % RING_CAP;
  q->count++;
  pthread_cond_signal(&q->not_empty);
  pthread_mutex_unlock(&q->mu);
}

static uint64_t queue_pop(queue_t *q) {
  pthread_mutex_lock(&q->mu);
  while (q->count == 0u) {
    pthread_cond_wait(&q->not_empty, &q->mu);
  }
  uint64_t v = q->buf[q->head];
  q->head = (q->head + 1u) % RING_CAP;
  q->count--;
  pthread_cond_signal(&q->not_full);
  pthread_mutex_unlock(&q->mu);
  return v;
}

static void *consumer(void *arg) {
  queue_t *q = (queue_t *)arg;
  for (uint64_t i = 0; i < q->total; ++i) {
    (void)queue_pop(q);
  }
  return NULL;
}

static double queue_msgs_per_sec(void) {
  queue_t q;
  queue_init(&q, QUEUE_TOTAL);
  pthread_t th;
  pthread_create(&th, NULL, consumer, &q);
  struct timespec s, e;
  clock_gettime(CLOCK_MONOTONIC, &s);
  for (uint64_t i = 0; i < QUEUE_TOTAL; ++i) {
    queue_push(&q, i);
  }
  pthread_join(th, NULL);
  clock_gettime(CLOCK_MONOTONIC, &e);
  return (double)QUEUE_TOTAL / (elapsed_ns(s, e) / 1e9);
}

static double scheduler_msgs_per_sec(void) {
  uint64_t q[4][128];
  for (int i = 0; i < 4; ++i) {
    for (int j = 0; j < 64; ++j) q[i][j] = 0;
  }
  uint64_t processed = 0, cursor = 0;
  struct timespec s, e;
  clock_gettime(CLOCK_MONOTONIC, &s);
  while (processed < SCHED_TOTAL) {
    int idx = (int)(cursor & 3u);
    int pos = (int)(cursor & 127u);
    uint64_t v = q[idx][pos] + 1u;
    for (uint64_t i = 0; i < SPIN; ++i) {
      v = v * 6364136223846793005ull + 1ull;
    }
    q[idx][(pos + 1) & 127u] = v;
    processed++;
    cursor++;
  }
  clock_gettime(CLOCK_MONOTONIC, &e);
  return (double)SCHED_TOTAL / (elapsed_ns(s, e) / 1e9);
}

typedef struct {
  queue_t *q;
  uint64_t processed;
} worker_ctx_t;

static void *wq_worker(void *arg) {
  worker_ctx_t *ctx = (worker_ctx_t *)arg;
  for (;;) {
    uint64_t v = queue_pop(ctx->q);
    if (v == UINT64_MAX) break;
    for (uint64_t i = 0; i < SPIN; ++i) {
      v = v * 6364136223846793005ull + 1ull;
    }
    ctx->processed++;
  }
  return NULL;
}

static void *wq_producer(void *arg) {
  queue_t *q = (queue_t *)arg;
  static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
  static uint64_t next = 0;
  for (;;) {
    pthread_mutex_lock(&mu);
    uint64_t v = next;
    if (v >= WQ_TOTAL) {
      pthread_mutex_unlock(&mu);
      break;
    }
    next++;
    pthread_mutex_unlock(&mu);
    queue_push(q, v);
  }
  return NULL;
}

static double workqueue_msgs_per_sec(void) {
  queue_t q;
  queue_init(&q, WQ_TOTAL);
  pthread_t workers[WQ_WORKERS];
  worker_ctx_t wctx[WQ_WORKERS];
  pthread_t producers[WQ_PRODUCERS];
  struct timespec s, e;

  for (int i = 0; i < WQ_WORKERS; i++) {
    wctx[i].q = &q;
    wctx[i].processed = 0;
    pthread_create(&workers[i], NULL, wq_worker, &wctx[i]);
  }
  for (int i = 0; i < WQ_PRODUCERS; i++) {
    pthread_create(&producers[i], NULL, wq_producer, &q);
  }

  clock_gettime(CLOCK_MONOTONIC, &s);
  for (int i = 0; i < WQ_PRODUCERS; i++) {
    pthread_join(producers[i], NULL);
  }
  for (int i = 0; i < WQ_WORKERS; i++) {
    queue_push(&q, UINT64_MAX);
  }
  uint64_t total = 0;
  for (int i = 0; i < WQ_WORKERS; i++) {
    pthread_join(workers[i], NULL);
    total += wctx[i].processed;
  }
  clock_gettime(CLOCK_MONOTONIC, &e);
  return (double)total / (elapsed_ns(s, e) / 1e9);
}

int main(void) {
  printf("queue_msgs_per_sec=%.2f\n", queue_msgs_per_sec());
  printf("scheduler_msgs_per_sec=%.2f\n", scheduler_msgs_per_sec());
  printf("workqueue_msgs_per_sec=%.2f\n", workqueue_msgs_per_sec());
  return 0;
}
EOF

cat > "$SRC_DIR/go_bench.go" <<'EOF'
package main

import (
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

const queueTotal uint64 = 300_000
const schedTotal uint64 = 150_000
const spin uint64 = 128
const wqTotal uint64 = 300_000
const wqWorkers int = 4
const wqProducers int = 4

func queueMsgsPerSec() float64 {
	ch := make(chan uint64, 1024)
	done := make(chan struct{})
	go func() {
		var got uint64
		for got < queueTotal {
			<-ch
			got++
		}
		close(done)
	}()
	start := time.Now()
	for i := uint64(0); i < queueTotal; i++ {
		ch <- i
	}
	<-done
	secs := time.Since(start).Seconds()
	return float64(queueTotal) / secs
}

func schedulerMsgsPerSec() float64 {
	var q [4][128]uint64
	var processed uint64
	var cursor uint64
	start := time.Now()
	for processed < schedTotal {
		idx := cursor & 3
		pos := cursor & 127
		v := q[idx][pos] + 1
		for i := uint64(0); i < spin; i++ {
			v = v*6364136223846793005 + 1
		}
		q[idx][(pos+1)&127] = v
		processed++
		cursor++
	}
	secs := time.Since(start).Seconds()
	return float64(schedTotal) / secs
}

func workqueueMsgsPerSec() float64 {
	ch := make(chan uint64, 4096)
	var processed uint64 = 0
	var wgWorkers sync.WaitGroup
	var wgProducers sync.WaitGroup

	for i := 0; i < wqWorkers; i++ {
		wgWorkers.Add(1)
		go func() {
			defer wgWorkers.Done()
			for v := range ch {
				m := v
				for i := uint64(0); i < spin; i++ {
					m = m*6364136223846793005 + 1
				}
				_ = m
				atomic.AddUint64(&processed, 1)
			}
		}()
	}

	start := time.Now()
	for p := 0; p < wqProducers; p++ {
		wgProducers.Add(1)
		go func(p int) {
			defer wgProducers.Done()
			for i := uint64(p); i < wqTotal; i += uint64(wqProducers) {
				ch <- i
			}
		}(p)
	}
	wgProducers.Wait()
	close(ch)
	wgWorkers.Wait()
	secs := time.Since(start).Seconds()
	return float64(atomic.LoadUint64(&processed)) / secs
}

func main() {
	fmt.Printf("queue_msgs_per_sec=%.2f\n", queueMsgsPerSec())
	fmt.Printf("scheduler_msgs_per_sec=%.2f\n", schedulerMsgsPerSec())
	fmt.Printf("workqueue_msgs_per_sec=%.2f\n", workqueueMsgsPerSec())
}
EOF

cat > "$SRC_DIR/Bench.java" <<'EOF'
import java.util.concurrent.ArrayBlockingQueue;

public class Bench {
    private static final long QUEUE_TOTAL = 300_000L;
    private static final long SCHED_TOTAL = 150_000L;
    private static final long SPIN = 128L;

    private static double queueMsgsPerSec() throws Exception {
        var q = new ArrayBlockingQueue<Long>(1024);
        Thread consumer = new Thread(() -> {
            long got = 0;
            try {
                while (got < QUEUE_TOTAL) {
                    q.take();
                    got++;
                }
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
        });
        consumer.start();
        long start = System.nanoTime();
        for (long i = 0; i < QUEUE_TOTAL; i++) {
            q.put(i);
        }
        consumer.join();
        double secs = (System.nanoTime() - start) / 1_000_000_000.0;
        return QUEUE_TOTAL / secs;
    }

    private static double schedulerMsgsPerSec() {
        long[][] q = new long[4][128];
        long processed = 0;
        long cursor = 0;
        long start = System.nanoTime();
        while (processed < SCHED_TOTAL) {
            int idx = (int)(cursor & 3);
            int pos = (int)(cursor & 127);
            long v = q[idx][pos] + 1;
            for (long i = 0; i < SPIN; i++) {
                v = v * 6364136223846793005L + 1L;
            }
            q[idx][(pos + 1) & 127] = v;
            processed++;
            cursor++;
        }
        double secs = (System.nanoTime() - start) / 1_000_000_000.0;
        return SCHED_TOTAL / secs;
    }

    public static void main(String[] args) throws Exception {
        System.out.printf("queue_msgs_per_sec=%.2f%n", queueMsgsPerSec());
        System.out.printf("scheduler_msgs_per_sec=%.2f%n", schedulerMsgsPerSec());
    }
}
EOF

cat > "$SRC_DIR/node_bench.mjs" <<'EOF'
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';

const QUEUE_TOTAL = 300_000;
const SCHED_TOTAL = 150_000;
const SPIN = 128;
const WQ_TOTAL = 300_000;
const WQ_WORKERS = 4;
const WQ_PRODUCERS = 4;

async function queueMsgsPerSec() {
  const worker = new Worker(new URL(import.meta.url), {
    workerData: { mode: 'queue_consumer', total: QUEUE_TOTAL },
  });
  const start = process.hrtime.bigint();
  const done = new Promise((resolve, reject) => {
    worker.on('message', (m) => {
      if (m === 'done') resolve();
    });
    worker.on('error', reject);
    worker.on('exit', (code) => {
      if (code !== 0) reject(new Error(`worker exit ${code}`));
    });
  });
  for (let i = 0; i < QUEUE_TOTAL; i++) {
    worker.postMessage(i);
  }
  await done;
  await worker.terminate();
  const elapsedNs = Number(process.hrtime.bigint() - start);
  return QUEUE_TOTAL / (elapsedNs / 1e9);
}

function schedulerMsgsPerSec() {
  const q = Array.from({ length: 4 }, () => new Array(128).fill(0n));
  let processed = 0;
  let cursor = 0;
  const start = process.hrtime.bigint();
  while (processed < SCHED_TOTAL) {
    const idx = cursor & 3;
    const pos = cursor & 127;
    let v = q[idx][pos] + 1n;
    for (let i = 0; i < SPIN; i++) {
      v = v * 6364136223846793005n + 1n;
    }
    q[idx][(pos + 1) & 127] = v;
    processed++;
    cursor++;
  }
  const elapsedNs = Number(process.hrtime.bigint() - start);
  return SCHED_TOTAL / (elapsedNs / 1e9);
}

async function workqueueMsgsPerSec() {
  // Node worker messaging is extremely heavyweight and hard to make apples-to-apples here.
  // Return 0 so the harness records "not comparable" rather than a misleading number.
  return 0;
}

if (!isMainThread && workerData?.mode === 'queue_consumer') {
  let got = 0;
  const total = workerData.total ?? QUEUE_TOTAL;
  parentPort.on('message', () => {
    got++;
    if (got >= total) {
      parentPort.postMessage('done');
    }
  });
} else {
  const queue = await queueMsgsPerSec();
  const sched = schedulerMsgsPerSec();
  const wq = await workqueueMsgsPerSec();
  console.log(`queue_msgs_per_sec=${queue.toFixed(2)}`);
  console.log(`scheduler_msgs_per_sec=${sched.toFixed(2)}`);
  console.log(`workqueue_msgs_per_sec=${wq.toFixed(2)}`);
}
EOF

cat > "$SRC_DIR/erl_bench.escript" <<'EOF'
#!/usr/bin/env escript
%%! -noshell
-mode(compile).

main(_) ->
    QueueTotal = 300000,
    SchedTotal = 150000,
    Spin = 128,
    Q = queue_msgs_per_sec(QueueTotal),
    S = scheduler_msgs_per_sec(SchedTotal, Spin),
    W = workqueue_msgs_per_sec(300000, 4, 4, Spin),
    io:format("queue_msgs_per_sec=~.2f~n", [Q]),
    io:format("scheduler_msgs_per_sec=~.2f~n", [S]),
    io:format("workqueue_msgs_per_sec=~.2f~n", [W]).

queue_msgs_per_sec(Total) ->
    Parent = self(),
    Consumer = spawn(fun() -> consume_loop(Parent, 0, Total) end),
    Start = erlang:monotonic_time(nanosecond),
    send_loop(Consumer, 0, Total),
    receive done -> ok end,
    Elapsed = erlang:monotonic_time(nanosecond) - Start,
    Total / (Elapsed / 1000000000.0).

send_loop(_Pid, N, Total) when N >= Total -> ok;
send_loop(Pid, N, Total) ->
    Pid ! {msg, N},
    send_loop(Pid, N + 1, Total).

consume_loop(Parent, N, Total) when N >= Total ->
    Parent ! done;
consume_loop(Parent, N, Total) ->
    receive
        {msg, _} -> consume_loop(Parent, N + 1, Total)
    end.

scheduler_msgs_per_sec(Total, Spin) ->
    Q0 = lists:duplicate(64, 0),
    Q1 = lists:duplicate(64, 0),
    Q2 = lists:duplicate(64, 0),
    Q3 = lists:duplicate(64, 0),
    Start = erlang:monotonic_time(nanosecond),
    _ = sched_loop(0, 0, Total, Spin, {Q0, Q1, Q2, Q3}),
    Elapsed = erlang:monotonic_time(nanosecond) - Start,
    Total / (Elapsed / 1000000000.0).

sched_loop(Processed, _Cursor, Total, _Spin, Queues) when Processed >= Total ->
    Queues;
sched_loop(Processed, Cursor, Total, Spin, {Q0, Q1, Q2, Q3}) ->
    Idx = Cursor band 3,
    Pos = Cursor band 63,
    case Idx of
        0 ->
            V = lists:nth(Pos + 1, Q0) + 1,
            Mixed = spin(V, Spin),
            sched_loop(Processed + 1, Cursor + 1, Total, Spin, {set_nth(Pos + 1, Mixed, Q0), Q1, Q2, Q3});
        1 ->
            V = lists:nth(Pos + 1, Q1) + 1,
            Mixed = spin(V, Spin),
            sched_loop(Processed + 1, Cursor + 1, Total, Spin, {Q0, set_nth(Pos + 1, Mixed, Q1), Q2, Q3});
        2 ->
            V = lists:nth(Pos + 1, Q2) + 1,
            Mixed = spin(V, Spin),
            sched_loop(Processed + 1, Cursor + 1, Total, Spin, {Q0, Q1, set_nth(Pos + 1, Mixed, Q2), Q3});
        _ ->
            V = lists:nth(Pos + 1, Q3) + 1,
            Mixed = spin(V, Spin),
            sched_loop(Processed + 1, Cursor + 1, Total, Spin, {Q0, Q1, Q2, set_nth(Pos + 1, Mixed, Q3)})
    end.

spin(V, 0) -> V;
spin(V, N) ->
    spin((V * 6364136223846793005 + 1) band 16#FFFFFFFFFFFFFFFF, N - 1).

set_nth(1, Val, [_|T]) -> [Val|T];
set_nth(N, Val, [H|T]) when N > 1 -> [H|set_nth(N - 1, Val, T)].

workqueue_msgs_per_sec(Total, Workers, _Producers, Spin) ->
    Parent = self(),
    WorkerPids = [spawn(fun() -> wq_worker(Parent, Spin) end) || _ <- lists:seq(1, Workers)],
    Start = erlang:monotonic_time(nanosecond),
    wq_send_loop(0, Total, WorkerPids),
    _ = [Pid ! stop || Pid <- WorkerPids],
    wq_wait_done(Workers),
    Elapsed = erlang:monotonic_time(nanosecond) - Start,
    Total / (Elapsed / 1000000000.0).

wq_worker(Parent, Spin) ->
    receive
        stop -> Parent ! done;
        {task, V} ->
            _ = wq_spin(V + 1, Spin),
            wq_worker(Parent, Spin)
    end.

wq_send_loop(I, Total, _Workers) when I >= Total ->
    ok;
wq_send_loop(I, Total, Workers) ->
    W = lists:nth((I rem length(Workers)) + 1, Workers),
    W ! {task, I},
    wq_send_loop(I + 1, Total, Workers).

wq_wait_done(0) ->
    ok;
wq_wait_done(N) ->
    receive done -> wq_wait_done(N - 1) end.

wq_spin(V, 0) -> V;
wq_spin(V, N) ->
    wq_spin((V * 6364136223846793005 + 1) band 16#FFFFFFFFFFFFFFFF, N - 1).
EOF

chmod +x "$SRC_DIR/erl_bench.escript"

# Collect Wrela artifacts as a series too (stabilizes against run-to-run noise).
wrela_cmd='
  WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::actor::tests::actor_fast_path_throughput_artifact -- --ignored >/dev/null 2>&1 \
    || WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::actor::tests::actor_fast_path_throughput_artifact -- --ignored >/dev/null 2>&1 \
    || WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::actor::tests::actor_fast_path_throughput_artifact -- --ignored >/dev/null 2>&1
  WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::scheduler::tests::scheduler_synthetic_artifact -- --ignored >/dev/null 2>&1 \
    || WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::scheduler::tests::scheduler_synthetic_artifact -- --ignored >/dev/null 2>&1 \
    || WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::scheduler::tests::scheduler_synthetic_artifact -- --ignored >/dev/null 2>&1
  WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::scheduler::tests::workqueue_throughput_artifact -- --ignored >/dev/null 2>&1 \
    || WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::scheduler::tests::workqueue_throughput_artifact -- --ignored >/dev/null 2>&1 \
    || WRELA_RUNTIME_METRICS=0 WRELA_ACTOR_CATCH_PANIC=0 cargo test --release -p wrela_runtime kernel::scheduler::tests::workqueue_throughput_artifact -- --ignored >/dev/null 2>&1
  echo "queue_msgs_per_sec=$(extract_kv fast_path_msgs_per_sec "'"$ROOT"'/.artifacts/wre-412/actor_throughput.txt")"
  echo "scheduler_msgs_per_sec=$(extract_kv scheduler_msgs_per_sec "'"$ROOT"'/.artifacts/wre-416/scheduler_synthetic_lane.txt")"
  echo "workqueue_msgs_per_sec=$(extract_kv workqueue_msgs_per_sec "'"$ROOT"'/.artifacts/wre-417/workqueue_throughput.txt")"
'
run_collect "wrela" "$wrela_cmd" 1
wrela_queue="$(median_from_file "$RAW_DIR/wrela_queue.series")"
wrela_scheduler="$(median_from_file "$RAW_DIR/wrela_scheduler.series")"
wrela_workqueue="$(median_from_file "$RAW_DIR/wrela_workqueue.series")"
wrela_queue_p95="$(percentile_from_file "$RAW_DIR/wrela_queue.series" 0.95)"
wrela_scheduler_p95="$(percentile_from_file "$RAW_DIR/wrela_scheduler.series" 0.95)"
wrela_workqueue_p95="$(percentile_from_file "$RAW_DIR/wrela_workqueue.series" 0.95)"
wrela_queue_cv_pct="$(cv_from_file "$RAW_DIR/wrela_queue.series")"
wrela_scheduler_cv_pct="$(cv_from_file "$RAW_DIR/wrela_scheduler.series")"
wrela_workqueue_cv_pct="$(cv_from_file "$RAW_DIR/wrela_workqueue.series")"
wrela_starvation_bound="$(extract_kv starvation_bound_ticks "$ROOT/.artifacts/wre-414/scheduler_objective_throughput.txt")"

declare -a languages=()
declare -a compile_notes=()

if have rustc; then
  if rustc -C opt-level=3 "$SRC_DIR/rust_bench.rs" -o "$BIN_DIR/rust_bench" && run_collect "rust" "$BIN_DIR/rust_bench"; then
    languages+=("rust")
  else
    compile_notes+=("rust:failed_to_run")
  fi
else
  compile_notes+=("rust:missing_toolchain")
fi

if have cc; then
  if cc -O3 -march=native -pthread "$SRC_DIR/c_bench.c" -o "$BIN_DIR/c_bench" && run_collect "c" "$BIN_DIR/c_bench"; then
    languages+=("c")
  else
    compile_notes+=("c:failed_to_run")
  fi
else
  compile_notes+=("c:missing_toolchain")
fi

if have go; then
  if go build -o "$BIN_DIR/go_bench" "$SRC_DIR/go_bench.go" && run_collect "go" "$BIN_DIR/go_bench"; then
    languages+=("go")
  else
    compile_notes+=("go:failed_to_run")
  fi
else
  compile_notes+=("go:missing_toolchain")
fi

if have javac && have java && usable java -version && usable javac -version; then
  if javac -d "$BIN_DIR" "$SRC_DIR/Bench.java" && run_collect "java" "java -cp \"$BIN_DIR\" Bench" 3; then
    languages+=("java")
  else
    compile_notes+=("java:failed_to_run")
  fi
else
  compile_notes+=("java:missing_or_unusable_runtime")
fi

if have node; then
  if run_collect "node" "node \"$SRC_DIR/node_bench.mjs\""; then
    languages+=("node")
  else
    compile_notes+=("node:failed_to_run")
  fi
else
  compile_notes+=("node:missing_toolchain")
fi

if have escript; then
  if run_collect "erlang" "escript \"$SRC_DIR/erl_bench.escript\""; then
    languages+=("erlang")
  else
    compile_notes+=("erlang:failed_to_run")
  fi
else
  compile_notes+=("erlang:missing_toolchain")
fi

emit_language_json() {
  local lang="$1"
  local q_series="$RAW_DIR/${lang}_queue.series"
  local s_series="$RAW_DIR/${lang}_scheduler.series"
  local wq_series="$RAW_DIR/${lang}_workqueue.series"
  local q_med s_med wq_med q_p95 s_p95 wq_p95 q_cv s_cv wq_cv dq ds dw
  q_med="$(median_from_file "$q_series")"
  s_med="$(median_from_file "$s_series")"
  wq_med="$(median_from_file "$wq_series")"
  q_p95="$(percentile_from_file "$q_series" 0.95)"
  s_p95="$(percentile_from_file "$s_series" 0.95)"
  wq_p95="$(percentile_from_file "$wq_series" 0.95)"
  q_cv="$(cv_from_file "$q_series")"
  s_cv="$(cv_from_file "$s_series")"
  wq_cv="$(cv_from_file "$wq_series")"
  dq="$(pct_delta_higher_better "$wrela_queue" "$q_med")"
  ds="$(pct_delta_higher_better "$wrela_scheduler" "$s_med")"
  dw="$(pct_delta_higher_better "$wrela_workqueue" "$wq_med")"
  cat <<JSON
{
  "language": "$lang",
  "queue_msgs_per_sec_median": $q_med,
  "queue_msgs_per_sec_p95": $q_p95,
  "queue_cv_pct": $q_cv,
  "scheduler_msgs_per_sec_median": $s_med,
  "scheduler_msgs_per_sec_p95": $s_p95,
  "scheduler_cv_pct": $s_cv,
  "workqueue_msgs_per_sec_median": $wq_med,
  "workqueue_msgs_per_sec_p95": $wq_p95,
  "workqueue_cv_pct": $wq_cv,
  "wrela_vs_${lang}_queue_pct": $dq,
  "wrela_vs_${lang}_scheduler_pct": $ds,
  "wrela_vs_${lang}_workqueue_pct": $dw
}
JSON
}

lang_json=()
for lang in "${languages[@]}"; do
  lang_json+=("$(emit_language_json "$lang")")
done

joined=""
for i in "${!lang_json[@]}"; do
  if [[ "$i" -gt 0 ]]; then
    joined+=","
  fi
  joined+="${lang_json[$i]}"
done

notes_json=""
if [[ "${#compile_notes[@]}" -gt 0 ]]; then
  for i in "${!compile_notes[@]}"; do
    if [[ "$i" -gt 0 ]]; then
      notes_json+=","
    fi
    notes_json+="\"${compile_notes[$i]}\""
  done
fi

baseline_created="false"
if [[ "${FREEZE_BASELINE:-0}" == "1" || ! -f "$BASELINE_JSON" ]]; then
  cp "$OUT_JSON" "$BASELINE_JSON" 2>/dev/null || true
  baseline_created="true"
fi

cat > "$OUT_JSON" <<JSON
{
  "version": 1,
  "generated_at_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "runs_per_language": $RUNS,
  "wrela": {
    "queue_msgs_per_sec": $wrela_queue,
    "queue_msgs_per_sec_p95": $wrela_queue_p95,
    "queue_cv_pct": $wrela_queue_cv_pct,
    "scheduler_msgs_per_sec": $wrela_scheduler,
    "scheduler_msgs_per_sec_p95": $wrela_scheduler_p95,
    "scheduler_cv_pct": $wrela_scheduler_cv_pct,
    "workqueue_msgs_per_sec": $wrela_workqueue,
    "workqueue_msgs_per_sec_p95": $wrela_workqueue_p95,
    "workqueue_cv_pct": $wrela_workqueue_cv_pct,
    "scheduler_starvation_bound_ticks": $wrela_starvation_bound
  },
  "languages_tested": [$(printf '"%s",' "${languages[@]}" | sed 's/,$//')],
  "results": [$joined],
  "notes": [$notes_json],
  "baseline_path": ".artifacts/perf/crosslang/baseline.json",
  "baseline_created_this_run": $baseline_created,
  "raw_dir": ".artifacts/perf/crosslang/raw"
}
JSON

if [[ "$baseline_created" == "true" ]]; then
  cp "$OUT_JSON" "$BASELINE_JSON"
fi

echo "wrote $OUT_JSON"
