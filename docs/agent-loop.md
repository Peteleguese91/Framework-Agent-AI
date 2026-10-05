# Agent loop

The loop is a persisted state machine: understand, plan, collect context, choose action, authorize, execute, observe, update memory, verify, and finish. A task ends only on verified completion, required user input, required approval, an unrecoverable error, cancellation, or a safety/budget limit.

Tool results are structured observations rather than chat text. Repeated equivalent failures trigger strategy review and ultimately loop protection. Verification requires inspecting changes and running the best available tests/build; missing verification is reported explicitly.
