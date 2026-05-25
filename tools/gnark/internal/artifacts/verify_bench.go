package artifacts

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"sort"
)

type VerifyBenchResultJSON struct {
	Curve                string  `json:"curve"`
	Circuit              string  `json:"circuit"`
	ClaimedStatementHash string  `json:"claimed_statement_hash"`
	LoadOrDecodeMS       float64 `json:"load_or_decode_ms"`
	PrepareMS            float64 `json:"prepare_ms,omitempty"`
	TranslateMS          float64 `json:"translate_ms,omitempty"`
	PrepareVKMS          float64 `json:"prepare_vk_ms,omitempty"`
	VerifyWarmupIters    int     `json:"verify_warmup_iterations"`
	VerifyMeasuredIters  int     `json:"verify_measured_iterations"`
	VerifyMeanMS         float64 `json:"verify_mean_ms"`
	VerifyMedianMS       float64 `json:"verify_median_ms"`
	VerifyMinMS          float64 `json:"verify_min_ms"`
	VerifyMaxMS          float64 `json:"verify_max_ms"`
}

func LoadArtifactJSON(path string) (*ArtifactJSON, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var artifacts ArtifactJSON
	if err := json.Unmarshal(data, &artifacts); err != nil {
		return nil, fmt.Errorf("decode %s: %w", path, err)
	}
	return &artifacts, nil
}

func WriteJSON(path string, value any) error {
	file, err := os.Create(path)
	if err != nil {
		return err
	}
	defer file.Close()

	encoder := json.NewEncoder(file)
	encoder.SetIndent("", "  ")
	return encoder.Encode(value)
}

func ComputeDurationStats(samples []float64) (mean, median, min, max float64) {
	if len(samples) == 0 {
		return 0, 0, 0, 0
	}

	total := 0.0
	min = math.MaxFloat64
	max = 0
	sorted := append([]float64(nil), samples...)
	sort.Float64s(sorted)
	for _, sample := range sorted {
		total += sample
		if sample < min {
			min = sample
		}
		if sample > max {
			max = sample
		}
	}
	mean = total / float64(len(sorted))
	mid := len(sorted) / 2
	if len(sorted)%2 == 0 {
		median = (sorted[mid-1] + sorted[mid]) / 2
	} else {
		median = sorted[mid]
	}
	return mean, median, min, max
}
