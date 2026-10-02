use super::*;

impl Workspace {
    pub(super) fn style_inputs(&self) -> [&Entity<TextInput>; 4] {
        [
            &self.colors.name,
            &self.colors.value,
            &self.colors.angle,
            &self.colors.position,
        ]
    }

    pub(super) fn finish_style_input(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= 2
            && self
                .colors
                .dialog
                .as_ref()
                .is_none_or(|d| d.gradient.is_none())
        {
            return;
        }
        let input = self.style_inputs()[index].clone();
        let value = input.read(cx).value();
        let valid = match index {
            0 => !value.trim().is_empty() && value.trim().chars().count() <= 200,
            1 => parse_color(&value).is_some(),
            _ => value.parse::<f32>().is_ok_and(|v| {
                v.is_finite() && (0. ..=if index == 2 { 360. } else { 100. }).contains(&v)
            }),
        };
        self.colors.error = (!valid).then(|| {
            t(match index {
                0 => "color-style-invalid-name",
                1 => "color-style-invalid",
                _ => "color-style-invalid-gradient",
            })
            .to_owned()
        });
        if valid {
            self.colors.baselines[index] = value;
        }
        cx.notify();
    }

    pub(in crate::editor) fn cancel_style_input(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self
            .style_inputs()
            .iter()
            .position(|input| input.focus_handle(cx).is_focused(window))
        else {
            return false;
        };
        let input = self.style_inputs()[index].clone();
        let before = self.colors.baselines[index].clone();
        if input.read(cx).value() == before {
            return false;
        }
        input.update(cx, |input, cx| input.set_value(before, cx));
        self.colors.error = None;
        cx.notify();
        true
    }
}
