window.BENCHMARK_DATA = {
  lastUpdate: 1621475680860,
  repoUrl: 'https://github.com/napi-rs/napi-rs',
  entries: {
    Benchmark: [
      {
        commit: {
          author: {
            email: 'lynweklm@gmail.com',
            name: 'LongYinan',
            username: 'Brooooooklyn',
          },
          committer: {
            email: 'noreply@github.com',
            name: 'GitHub',
            username: 'web-flow',
          },
          distinct: true,
          id: '9b7b6b0eaffe652c332e2b1f6e9ab916f46c3696',
          message: 'Merge pull request #568 from orangecms/patch-1',
          timestamp: '2021-05-20T09:50:38+08:00',
          tree_id: 'c65f764a84da2a69f0f918f85e95e85919588274',
          url: 'https://github.com/napi-rs/napi-rs/commit/9b7b6b0eaffe652c332e2b1f6e9ab916f46c3696',
        },
        date: 1621475679560,
        tool: 'benchmarkjs',
        benches: [
          {
            name: 'noop#napi-rs',
            value: 54455062,
            range: '±0.11%',
            unit: 'ops/sec',
            extra: '99 samples',
          },
          {
            name: 'noop#JavaScript',
            value: 715133735,
            range: '±0.1%',
            unit: 'ops/sec',
            extra: '98 samples',
          },
          {
            name: 'Plus number#napi-rs',
            value: 22051558,
            range: '±0.15%',
            unit: 'ops/sec',
            extra: '96 samples',
          },
          {
            name: 'Plus number#JavaScript',
            value: 712738538,
            range: '±0.13%',
            unit: 'ops/sec',
            extra: '94 samples',
          },
          {
            name: 'Create buffer#napi-rs',
            value: 90458,
            range: '±24.99%',
            unit: 'ops/sec',
            extra: '68 samples',
          },
          {
            name: 'Create buffer#JavaScript',
            value: 57440,
            range: '±113.31%',
            unit: 'ops/sec',
            extra: '94 samples',
          },
          {
            name: 'createArray#createArrayJson',
            value: 32804,
            range: '±0.15%',
            unit: 'ops/sec',
            extra: '95 samples',
          },
          {
            name: 'createArray#create array for loop',
            value: 7894,
            range: '±0.13%',
            unit: 'ops/sec',
            extra: '99 samples',
          },
          {
            name: 'createArray#create array with serde trait',
            value: 7924,
            range: '±0.57%',
            unit: 'ops/sec',
            extra: '99 samples',
          },
          {
            name: 'getArrayFromJs#get array from json string',
            value: 16935,
            range: '±0.28%',
            unit: 'ops/sec',
            extra: '95 samples',
          },
          {
            name: 'getArrayFromJs#get array from serde',
            value: 10665,
            range: '±0.09%',
            unit: 'ops/sec',
            extra: '97 samples',
          },
          {
            name: 'getArrayFromJs#get array with for loop',
            value: 12912,
            range: '±0.18%',
            unit: 'ops/sec',
            extra: '97 samples',
          },
          {
            name: 'Get Set property#Get Set from native#u32',
            value: 463717,
            range: '±3.06%',
            unit: 'ops/sec',
            extra: '85 samples',
          },
          {
            name: 'Get Set property#Get Set from JavaScript#u32',
            value: 386281,
            range: '±2.85%',
            unit: 'ops/sec',
            extra: '88 samples',
          },
          {
            name: 'Get Set property#Get Set from native#string',
            value: 414122,
            range: '±3.09%',
            unit: 'ops/sec',
            extra: '82 samples',
          },
          {
            name: 'Get Set property#Get Set from JavaScript#string',
            value: 377660,
            range: '±2.89%',
            unit: 'ops/sec',
            extra: '89 samples',
          },
          {
            name: 'Async task#spawn task',
            value: 36449,
            range: '±1.34%',
            unit: 'ops/sec',
            extra: '85 samples',
          },
          {
            name: 'Async task#thread safe function',
            value: 1218,
            range: '±178.31%',
            unit: 'ops/sec',
            extra: '81 samples',
          },
        ],
      },
    ],
  },
}
