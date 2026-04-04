PROVIDER_COPY = {
    "local": (
        "Local mode is the default. It optimizes for privacy, offline use after download, "
        "and the feeling that the app belongs to the machine."
    ),
    "cloud": (
        "Cloud mode is for users who want lighter local requirements or better accuracy on weak hardware. "
        "It should be clearly labeled as network-backed and opt-in."
    ),
}


LOCAL_MODELS = [
    {
        "name": "Swift Whisper Small",
        "summary": "Balanced local model for laptops that need usable latency and sane accuracy.",
        "pill": "Default",
    },
    {
        "name": "Swift Whisper Tiny",
        "summary": "Lower-end hardware option with faster turnaround and weaker transcript fidelity.",
        "pill": "Fast",
    },
]


CLOUD_MODELS = [
    {
        "name": "Relay Realtime",
        "summary": "Streaming cloud backend tuned for low-latency dictation when local hardware is weak.",
        "pill": "Fastest",
    },
    {
        "name": "Relay Accurate",
        "summary": "Higher-quality remote backend for users who value transcript quality over pure latency.",
        "pill": "Best Quality",
    },
]
