#!/bin/bash

# Check if the process is running
if ! pgrep -f "thresh" > /dev/null; then
    echo "Thresh process is not running"
    exit 1
fi


# Check if the process is consuming too much memory (adjust threshold as needed)
MEMORY_USAGE=$(ps -o %mem= -p $(pgrep -f "thresh"))
if (( $(echo "$MEMORY_USAGE > 80.0" | bc -l) )); then
    echo "Memory usage too high: $MEMORY_USAGE%"
    exit 1
fi

# All checks passed
echo "Health check passed"
exit 0
