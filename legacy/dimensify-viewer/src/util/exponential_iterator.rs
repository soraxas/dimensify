#[derive(Debug)]
/// An iterator that generates an exponentially increasing
/// sequence of values.
pub struct ExponentialIterator {
    current: f32,
    max: f32,
    initial: f32,
    steps: usize,
    step: usize,
}

impl ExponentialIterator {
    pub fn new(initial: f32, max: f32, steps: usize) -> Self {
        ExponentialIterator {
            current: initial,
            max,
            initial,
            steps,
            step: 0,
        }
    }

    fn scale_factor(&self) -> f32 {
        // We will calculate the ratio using the formula to get exponential growth
        // The scale factor is applied step by step to generate an exponentially increasing sequence
        (self.max / self.initial).powf(1.0 / (self.steps as f32 - 1.0))
    }
}

impl Iterator for ExponentialIterator {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.step >= self.steps {
            return None; // Stop iteration after the last value
        }

        let current_value = self.current;

        // If we're not at the last step, multiply by the scale factor to exponentially increase
        if self.step < self.steps - 1 {
            self.current *= self.scale_factor(); // Exponentially increase
        } else {
            // On the last step, set the value to max exactly
            self.current = self.max;
        }

        self.step += 1;

        Some(current_value)
    }
}
