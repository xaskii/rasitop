from __future__ import annotations

import argparse
import json
from pathlib import Path

from recording import RecordingError, load_recording, summarize


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate and summarize rasitop CSV output")
    parser.add_argument("recording", type=Path)
    parser.add_argument(
        "--require-clean",
        action="store_true",
        help="fail when any row reports an error flag",
    )
    args = parser.parse_args()

    try:
        summary = summarize(load_recording(args.recording))
        if args.require_clean and summary.error_flags != 0:
            raise RecordingError(f"recording reports error flags {summary.error_flags:#x}")
    except RecordingError as error:
        parser.error(str(error))

    print(json.dumps(summary.as_json(), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
