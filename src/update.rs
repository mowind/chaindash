use num_rational::Ratio;

use crate::app::Widgets;

pub trait UpdatableWidget {
    fn update(&mut self);
    fn get_update_interval(&self) -> Ratio<u64>;
}

pub fn update_widgets(widgets: &mut Widgets, seconds: Ratio<u64>) {
    let mut widgets_to_update: Vec<&mut (dyn UpdatableWidget)> =
        vec![&mut widgets.txs, &mut widgets.time];

    for widget in widgets_to_update {
        if seconds % widget.get_update_interval() == Ratio::from_integer(0) {
            widget.update();
        }
    }
}
