# Pipeline Flow Observability and Runtime Controls — Launch

The increment is complete and launch-ready. Stream-mode operators can now follow one-based chunk numbers from Input Queue through Workers and Pending Sort to Output Ready. Level, Throttle, Chunk, and Workers controls are visible and adjustable; chunk sizing applies safely to future blocks and worker reductions do not interrupt in-flight work.

Compression defaults to maximum level 9 across modes while explicit overrides remain available. `-T` works with piped stdin when stderr is an interactive terminal. The dashboard retains its tested 80x22 minimum and now fills larger terminals, using extra width for the pipeline and extra height for live logs while keeping controls anchored consistently.

Delivery status: 9/9 tasks complete; Trin QA, Smith UX, and Morpheus architecture/code gates approved. Residual debt is limited to the missing documented `make judge-trace` target and the known environment-sensitive `/dev/tty` fixture.
