# Generate the fixed, non-private Chinese fixture used by probe-provider-latency.py.
# Windows desktop speech synthesis is local; no microphone or network is used.
param([string]$Voice = 'Microsoft Huihui Desktop')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$fixtureDirectory = Join-Path (Split-Path $PSScriptRoot) 'target/latency-probe'
New-Item -ItemType Directory -Force -Path $fixtureDirectory | Out-Null
$text = '今天我们测试语音输入的响应速度。用户说完一段话以后，希望系统尽快把准确的文字输入到当前窗口。第一步检查录音文件的长度和大小，第二步测量语音识别服务的处理时间，第三步测量文字整理服务的响应时间。测试过程中使用固定的公开文本，不包含真实用户的私人信息。我们会分别记录每个请求的开始时间和结束时间，比较不同方案的速度和识别结果。最终目标是让日常输入更加流畅，同时保持原有的意思，正确处理中文标点和常见的技术词汇。'
foreach ($sampleRate in @(48000, 16000)) {
    $synth = [System.Speech.Synthesis.SpeechSynthesizer]::new()
    try {
        $synth.SelectVoice($Voice)
        $synth.Rate = 0
        $format = [System.Speech.AudioFormat.SpeechAudioFormatInfo]::new(
            $sampleRate,
            [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
            [System.Speech.AudioFormat.AudioChannel]::Mono
        )
        $path = Join-Path $fixtureDirectory "synthetic-$sampleRate.wav"
        $synth.SetOutputToWaveFile($path, $format)
        $synth.Speak($text)
    } finally {
        $synth.Dispose()
    }
    Get-Item -LiteralPath $path | Select-Object Name, Length
}
