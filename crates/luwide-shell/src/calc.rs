//! Calculadora (lógica pura, testável sem GUI).

#[derive(Debug, Clone, Default)]
pub struct Calculator {
    display: String,
    acc: Option<f64>,
    op: Option<char>,
    fresh: bool, // próximo dígito substitui o display
}

impl Calculator {
    pub fn new() -> Self {
        Self { display: "0".into(), ..Default::default() }
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    fn value(&self) -> f64 {
        self.display.replace(',', ".").parse().unwrap_or(0.0)
    }

    fn set_display(&mut self, v: f64) {
        if !v.is_finite() {
            self.display = "Erro".into();
            self.acc = None;
            self.op = None;
            self.fresh = true;
            return;
        }
        let text = if v.fract() == 0.0 && v.abs() < 1e15 {
            format!("{}", v as i64)
        } else {
            format!("{v:.10}").trim_end_matches('0').trim_end_matches('.').to_string()
        };
        self.display = if text.is_empty() { "0".into() } else { text };
    }

    /// Pressiona uma tecla: '0'..'9', '.', '+', '-', '*', '/', '=', 'C' (limpa), 'B' (apaga).
    pub fn press(&mut self, key: char) {
        match key {
            '0'..='9' => {
                if self.fresh || self.display == "0" || self.display == "Erro" {
                    self.display = key.to_string();
                    self.fresh = false;
                } else if self.display.len() < 16 {
                    self.display.push(key);
                }
            }
            '.' => {
                if self.fresh || self.display == "Erro" {
                    self.display = "0.".into();
                    self.fresh = false;
                } else if !self.display.contains('.') {
                    self.display.push('.');
                }
            }
            '+' | '-' | '*' | '/' => {
                self.apply_pending();
                self.acc = Some(self.value());
                self.op = Some(key);
                self.fresh = true;
            }
            '=' => {
                self.apply_pending();
                self.op = None;
                self.fresh = true;
            }
            'C' => *self = Self::new(),
            'B' => {
                if !self.fresh && self.display.len() > 1 {
                    self.display.pop();
                } else {
                    self.display = "0".into();
                }
            }
            _ => {}
        }
    }

    fn apply_pending(&mut self) {
        if let (Some(acc), Some(op)) = (self.acc, self.op) {
            let v = self.value();
            let result = match op {
                '+' => acc + v,
                '-' => acc - v,
                '*' => acc * v,
                '/' => {
                    if v == 0.0 {
                        self.display = "Erro".into();
                        self.acc = None;
                        self.op = None;
                        self.fresh = true;
                        return;
                    }
                    acc / v
                }
                _ => v,
            };
            self.set_display(result);
            self.acc = Some(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(keys: &str) -> String {
        let mut c = Calculator::new();
        for k in keys.chars() {
            c.press(k);
        }
        c.display().to_string()
    }

    #[test]
    fn aritmetica_basica() {
        assert_eq!(run("2+3="), "5");
        assert_eq!(run("7*6="), "42");
        assert_eq!(run("10/4="), "2.5");
        assert_eq!(run("9-12="), "-3");
        assert_eq!(run("2+3+4="), "9");
        assert_eq!(run("1.5*2="), "3");
    }

    #[test]
    fn Divisão_por_zero_e_limpar() {
        assert_eq!(run("5/0="), "Erro");
        assert_eq!(run("123C"), "0");
        assert_eq!(run("12B"), "1");
    }
}
